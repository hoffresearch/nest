//! `MmapNestFile::inspect_json`: the `nest inspect --json` document. Kept out
//! of `mmap_file.rs` so that file stays under the 300-line crate guard; this
//! is pure presentation (re-parse the mmap, dump header + section table +
//! manifest + hashes + simd backend), no search math.

use nest_format::NestError;
use nest_format::layout::SECTION_CHUNKS_CANONICAL;
use nest_format::reader::NestView;
use nest_format::sections::decode_chunks_canonical;

use crate::error::RuntimeError;
use crate::mmap_file::MmapNestFile;

impl MmapNestFile {
    /// lDecode the stored canonical text for every chunk, in file order. this
    /// is the TIER-1 citation text, the same bytes `nest cite` returns, NOT
    /// the original source bytes. re-parses the mmap and decodes the
    /// chunks_canonical (0x02) section (handles zstd / txt_streams / dict /
    /// fsst transparently). used by the agent-native `retrieve()` to attach
    /// cited text to each hit.
    pub fn canonical_texts(&self) -> Result<Vec<String>, RuntimeError> {
        let view = NestView::from_bytes(&self._mmap)?;
        let n = view.header.n_chunks as usize;
        decode_chunks_canonical(&view.decoded_section(SECTION_CHUNKS_CANONICAL)?, n)
            .map_err(RuntimeError::Format)
    }

    /// lThe per-chunk ids in file order, parallel to `canonical_texts()`.
    /// lets a caller build a chunk_id -> canonical-text map without re-reading
    /// the file (the ids are already decoded and held at open time).
    pub fn chunk_ids(&self) -> &[String] {
        &self.chunk_ids
    }

    /// lRe-parse the mmap and return a JSON document mirroring `nest
    /// inspect`: header fields, section table entries, manifest, hashes,
    /// and the runtime SIMD backend.
    pub fn inspect_json(&self) -> Result<String, RuntimeError> {
        let view = NestView::from_bytes(&self._mmap)?;
        let magic = std::str::from_utf8(&view.header.magic)
            .unwrap_or("")
            .to_string();
        let sections: Vec<serde_json::Value> = view
            .section_table
            .iter()
            .map(|e| {
                let name = nest_format::layout::section_name(e.section_id).unwrap_or("unknown");
                serde_json::json!({
                    "section_id": e.section_id,
                    "name": name,
                    "encoding": e.encoding,
                    "offset": e.offset,
                    "size": e.size,
                    "checksum": hex::encode(e.checksum),
                })
            })
            .collect();
        let doc = serde_json::json!({
            "magic": magic,
            "version_major": view.header.version_major,
            "version_minor": view.header.version_minor,
            "format_version": view.manifest.format_version,
            "schema_version": view.manifest.schema_version,
            "embedding_dim": view.header.embedding_dim,
            "n_chunks": view.header.n_chunks,
            "n_embeddings": view.header.n_embeddings,
            "file_size": view.header.file_size,
            "manifest": view.manifest,
            "sections": sections,
            "file_hash": view.file_hash_hex(),
            "content_hash": view.content_hash_hex()?,
            "simd_backend": self.simd_backend().name(),
        });
        serde_json::to_string(&doc).map_err(|e| RuntimeError::Format(NestError::Json(e)))
    }
}
