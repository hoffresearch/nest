//! `MmapNestFile`: owns the mmap, parses section metadata once, exposes
//! search/inspect entry points. Search math lives in `super::search`;
//! dtype-agnostic vector materialization lives in `super::materialize`.

use std::path::Path;

use memmap2::Mmap;
use nest_format::NestError;
use nest_format::layout::{
    SECTION_BM25_INDEX, SECTION_CHUNK_IDS, SECTION_CHUNKS_ORIGINAL_SPANS, SECTION_EMBEDDINGS,
    SECTION_GRAPH_ADJACENCY, SECTION_HNSW_INDEX,
};
use nest_format::reader::NestView;
use nest_format::sections::{OriginalSpan, decode_chunk_ids, decode_chunks_original_spans};

use crate::ann;
use crate::bm25;
use crate::error::RuntimeError;
use crate::graph::CsrIndex;
use crate::materialize::PackedVectors;
use crate::rerank::FpSlab;
use crate::simd::{self, SimdBackend};

/// lRuntime view of the embeddings section dtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DType {
    Float32,
    Float16,
    Int8,
    Int4,
}

impl DType {
    pub(crate) fn from_str(s: &str) -> Result<Self, RuntimeError> {
        match s {
            "float32" => Ok(Self::Float32),
            "float16" => Ok(Self::Float16),
            "int8" => Ok(Self::Int8),
            "int4" => Ok(Self::Int4),
            other => Err(RuntimeError::Format(NestError::UnsupportedDType(
                other.into(),
            ))),
        }
    }
    /// lNominal on-disk bytes per stored embedding value. int4 packs two
    /// codes per byte (rounds to 0 here); the exact section size, with the
    /// f16 group scales, is `expected_embeddings_size`.
    pub fn bytes_per_value(self) -> usize {
        match self {
            Self::Float32 => 4,
            Self::Float16 => 2,
            Self::Int8 => 1,
            Self::Int4 => 0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Float32 => "float32",
            Self::Float16 => "float16",
            Self::Int8 => "int8",
            Self::Int4 => "int4",
        }
    }
}

pub struct MmapNestFile {
    pub(crate) _mmap: Mmap,
    pub(crate) embedding_dim: usize,
    pub(crate) n_embeddings: usize,
    pub(crate) dtype: DType,
    /// lByte offset (within the mmap) of the embeddings section payload.
    pub(crate) embeddings_offset: usize,
    /// lTotal physical bytes of the embeddings section.
    pub(crate) embeddings_size: usize,
    /// lOptional full-precision rerank slab (`embeddings_fp`, 0x09): when
    /// present the exact rerank reads it instead of the stored dtype slab.
    pub(crate) embeddings_fp: Option<FpSlab>,
    pub(crate) chunk_ids: Vec<String>,
    pub(crate) spans: Vec<OriginalSpan>,
    pub(crate) embedding_model: String,
    pub(crate) file_hash: String,
    pub(crate) content_hash: String,
    /// lOptional ANN index. Built from the HNSW section payload at open
    /// time (eager: build cost is paid once, queries get fast path).
    pub(crate) ann_index: Option<ann::HnswIndex>,
    /// lOptional BM25 index. Mostly tiny ints; deserialized eagerly.
    pub(crate) bm25_index: Option<bm25::Bm25Index>,
    /// lOptional chunk-to-chunk graph (graph_adjacency 0x0C). Opened behind
    /// the manifest `graph_present` capability, like ann/bm25. A candidate
    /// generator only: its frontier feeds the exact rerank, never a score.
    pub(crate) graph_index: Option<CsrIndex>,
    /// lOptional generic metadata inverted index (0x17). Opened by section
    /// presence like ann/bm25. Powers `search_filtered` (see the meta module).
    pub(crate) meta_index: Option<crate::meta::MetaIndex>,
    /// lWhat the manifest says the search path is. The runtime honors
    /// this at search time.
    pub(crate) declared_index_type: String,
    pub(crate) declared_score_type: String,
}

impl MmapNestFile {
    pub fn open(path: &Path) -> Result<Self, RuntimeError> {
        let file = std::fs::File::open(path)?;
        // lSAFETY: `file` is a valid open read-only handle we own for the map's
        // lifetime; truncating/mutating the backing file while mapped is UB (SIGBUS).
        let mmap = unsafe { Mmap::map(&file)? };
        let view = NestView::from_bytes(&mmap)?;
        view.validate_embeddings_values()?;

        let dim = view.header.embedding_dim as usize;
        let n = view.header.n_embeddings as usize;
        let dtype = DType::from_str(&view.manifest.dtype)?;

        let entry = view.entry(SECTION_EMBEDDINGS)?;
        let embeddings_offset = entry.offset as usize;
        let embeddings_size = entry.size as usize;

        // lOptional full-precision rerank slab (0x09); see rerank::FpSlab.
        let embeddings_fp = FpSlab::detect(&view.section_table, n, dim)?;

        // lDecoded chunk_ids / spans (handles zstd transparently).
        let chunk_ids = decode_chunk_ids(&view.decoded_section(SECTION_CHUNK_IDS)?, n)?;
        let spans =
            decode_chunks_original_spans(&view.decoded_section(SECTION_CHUNKS_ORIGINAL_SPANS)?, n)?;

        // lOptional ANN section. Materialize f32 vectors from the
        // embeddings section so the graph can compute distances at
        // search time independent of the on-disk dtype.
        let ann_index = if view
            .section_table
            .iter()
            .any(|e| e.section_id == SECTION_HNSW_INDEX)
        {
            let bytes = view.decoded_section(SECTION_HNSW_INDEX)?;
            let mut idx = ann::HnswIndex::from_bytes(&bytes, n, dim)?;
            let emb_bytes = view.get_section_data(SECTION_EMBEDDINGS)?;
            // lKeep the vectors in their on-disk packing (no n*dim*4 f32
            // expansion): the graph decodes one row at a time on demand.
            let store = PackedVectors::from_section(&view.manifest.dtype, emb_bytes, n, dim)?;
            idx.attach_store(store);
            Some(idx)
        } else {
            None
        };

        let bm25_index = if view
            .section_table
            .iter()
            .any(|e| e.section_id == SECTION_BM25_INDEX)
        {
            let bytes = view.decoded_section(SECTION_BM25_INDEX)?;
            Some(bm25::Bm25Index::from_bytes(&bytes)?)
        } else {
            None
        };

        // lOptional meta_index section (0x17); opened in the meta module by
        // section presence (no manifest flag), like bm25/hnsw, so a file
        // carrying the index keeps the SAME content_hash as one without it.
        let meta_index = crate::meta::open(&view)?;

        // lOptional graph_adjacency section (0x0C). gated behind the additive
        // `graph_present` capability (delivered via Option<CapabilitiesExt>),
        // mirroring how ann/bm25 are opened. raw payload; the csr bitpacks its
        // own integer columns with intpack internally.
        let graph_present = view
            .manifest
            .capabilities_ext
            .as_ref()
            .and_then(|e| e.graph_present)
            .unwrap_or(false);
        let graph_index = if graph_present
            && view
                .section_table
                .iter()
                .any(|e| e.section_id == SECTION_GRAPH_ADJACENCY)
        {
            let bytes = view.decoded_section(SECTION_GRAPH_ADJACENCY)?;
            Some(CsrIndex::from_bytes(&bytes, n)?)
        } else {
            None
        };

        let embedding_model = view.manifest.embedding_model.clone();
        let declared_index_type = view.manifest.index_type.clone();
        let declared_score_type = view.manifest.score_type.clone();
        let file_hash = view.file_hash_hex();
        let content_hash = view.content_hash_hex()?;
        drop(view);

        Ok(Self {
            _mmap: mmap,
            embedding_dim: dim,
            n_embeddings: n,
            dtype,
            embeddings_offset,
            embeddings_size,
            embeddings_fp,
            chunk_ids,
            spans,
            embedding_model,
            file_hash,
            content_hash,
            ann_index,
            bm25_index,
            graph_index,
            meta_index,
            declared_index_type,
            declared_score_type,
        })
    }

    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
    pub fn n_embeddings(&self) -> usize {
        self.n_embeddings
    }
    pub fn dtype(&self) -> DType {
        self.dtype
    }
    pub fn file_hash(&self) -> &str {
        &self.file_hash
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn simd_backend(&self) -> SimdBackend {
        simd::detect_backend()
    }
    pub fn declared_index_type(&self) -> &str {
        &self.declared_index_type
    }
    pub fn declared_score_type(&self) -> &str {
        &self.declared_score_type
    }
    pub fn has_ann(&self) -> bool {
        self.ann_index.is_some()
    }
    pub fn has_bm25(&self) -> bool {
        self.bm25_index.is_some()
    }
    pub fn has_graph(&self) -> bool {
        self.graph_index.is_some()
    }

    /// lRe-run all reader-side validation. The file was already
    /// validated at `open()` time, but callers can invoke this
    /// explicitly to detect tampering after the fact (e.g. the mmap
    /// pages got swapped under the runtime).
    pub fn revalidate(&self) -> Result<(), RuntimeError> {
        let view = NestView::from_bytes(&self._mmap)?;
        view.validate_embeddings_values()?;
        let _ = view.search_contract()?;
        Ok(())
    }

    pub(crate) fn embeddings_bytes(&self) -> &[u8] {
        &self._mmap[self.embeddings_offset..self.embeddings_offset + self.embeddings_size]
    }

    /// lThe full-precision rerank slab + its dtype, if an `embeddings_fp`
    /// (0x09) section is present. The rerank handle prefers this over the
    /// stored dtype slab so a sub-int8 corpus still returns a real cosine.
    pub(crate) fn embeddings_fp_slab(&self) -> Option<(&[u8], DType)> {
        self.embeddings_fp
            .map(|fp| (&self._mmap[fp.offset..fp.offset + fp.size], fp.dtype))
    }

    /// lHint to the OS that the mmap pages won't be needed soon. The
    /// next read will fault them back in from disk.
    ///
    /// **Caveat:** this is `posix_madvise(MADV_DONTNEED)` — an
    /// approximation of cold cache, NOT a guarantee. The OS may
    /// ignore the hint, keep pages around for prefetch, or return
    /// them from the kernel's page cache anyway. Use it for
    /// "madvise-cold" benchmarks (a useful upper bound on the warm
    /// case) but don't claim it's equivalent to a fresh boot. Real
    /// cold-cache benchmarks need `purge` (macOS) or `echo 3 >
    /// /proc/sys/vm/drop_caches` (Linux), both of which require
    /// privileged operations.
    #[cfg(unix)]
    pub fn madvise_cold(&self) {
        use std::ffi::c_void;
        // lSAFETY: passing a valid mmap pointer + length. POSIX_MADV_DONTNEED
        // does not invalidate or move the mapping; we still hold the Mmap
        // and can read from it as before.
        unsafe {
            libc::posix_madvise(
                self._mmap.as_ptr() as *mut c_void,
                self._mmap.len(),
                libc::POSIX_MADV_DONTNEED,
            );
        }
    }

    /// lNo-op on non-Unix platforms.
    #[cfg(not(unix))]
    pub fn madvise_cold(&self) {}
}
