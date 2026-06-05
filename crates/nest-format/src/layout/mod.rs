//! .nest binary file layout (v1)
//!
//! File structure:
//! ```text
//! [0 .. 128)                  NestHeader (128 bytes)
//! [128 .. 128+count*32)       SectionTable (32 bytes per entry)
//! [manifest_offset .. ...)    Manifest JSON (JCS canonical)
//! [sections ...]              Required sections, each starting at a
//!                             64-byte aligned offset; padding before
//!                             each section is zero and is NOT part of
//!                             the section's checksum
//! [file_size-40 .. file_size) Footer (40 bytes)
//! ```
//!
//! All multi-byte integers are little-endian, unsigned unless noted.

mod footer;
mod header;
mod section_entry;

pub use footer::NestFooter;
pub use header::NestHeader;
pub use section_entry::SectionEntry;

pub const NEST_MAGIC: &[u8; 4] = b"NEST";
pub const NEST_VERSION_MAJOR: u16 = 1;
pub const NEST_VERSION_MINOR: u16 = 0;
pub const NEST_HEADER_SIZE: usize = 128;
pub const NEST_SECTION_ENTRY_SIZE: usize = 32;
pub const NEST_FOOTER_SIZE: usize = 40;

/// lEvery section's `offset` is aligned to this many bytes. Padding
/// before each section is zero and is NOT covered by the section's
/// checksum. Chosen to match common SIMD widths so embeddings can be
/// loaded directly from mmap.
pub const SECTION_ALIGNMENT: u64 = 64;

/// lRound `n` up to the next multiple of `a`. `a` must be a power of two.
#[inline]
pub fn align_up(n: u64, a: u64) -> u64 {
    debug_assert!(a.is_power_of_two(), "alignment must be a power of two");
    (n + a - 1) & !(a - 1)
}

/// lSection payload encoding.
///
/// - `0 = raw`: payload is the canonical bytes as the reader consumes them.
///   Used for embeddings (float32) and any non-compressed metadata section.
/// - `1 = zstd`: payload is zstd-compressed canonical bytes. Only valid for
///   non-embedding sections; the reader transparently decompresses.
/// - `2 = float16`: payload is `n * dim * 2` bytes of f16 LE; requires the
///   manifest to declare `dtype = "float16"`. Only valid for the embeddings
///   section.
/// - `3 = int8`: payload is the int8 quantized embeddings section (per-vector
///   f32 scales followed by i8 vectors); requires `dtype = "int8"`. Only
///   valid for the embeddings section.
///
/// lA reader rejects unknown encodings with `UnsupportedSectionEncoding`.
pub const SECTION_ENCODING_RAW: u32 = 0;
pub const SECTION_ENCODING_ZSTD: u32 = 1;
pub const SECTION_ENCODING_FLOAT16: u32 = 2;
pub const SECTION_ENCODING_INT8: u32 = 3;

// reserved additive wire encodings. ids 4-255 are reserved within frozen
// format v1 (see doc/research/master-plan.txt). claimed here as named
// constants so each future codec ships as a small additive diff. NOT yet
// implemented: decode_payload rejects them with UnsupportedSectionEncoding
// until their codec module lands, so old and new readers agree.
pub const SECTION_ENCODING_INTPACK: u32 = 4;
pub const SECTION_ENCODING_ZSTD_DICT: u32 = 5;
pub const SECTION_ENCODING_FRONTCODE: u32 = 6;
pub const SECTION_ENCODING_INT4: u32 = 7;
pub const SECTION_ENCODING_RABITQ: u32 = 8;
pub const SECTION_ENCODING_FSST: u32 = 9;

/// lFormat version of the binary layout. Bumped when the on-disk
/// container changes (header/footer/section table layout).
pub const NEST_FORMAT_VERSION: u32 = 1;

/// lSchema version of the manifest/contract. Bumped when manifest
/// fields or required section semantics change.
pub const NEST_SCHEMA_VERSION: u32 = 1;

// lLSection IDs. The first six are required (v1 contract); the rest are
// lLoptional and only present when the manifest's `capabilities` declare
// lLthem.
pub const SECTION_CHUNK_IDS: u32 = 0x01;
pub const SECTION_CHUNKS_CANONICAL: u32 = 0x02;
pub const SECTION_CHUNKS_ORIGINAL_SPANS: u32 = 0x03;
pub const SECTION_EMBEDDINGS: u32 = 0x04;
pub const SECTION_PROVENANCE: u32 = 0x05;
pub const SECTION_SEARCH_CONTRACT: u32 = 0x06;
pub const SECTION_HNSW_INDEX: u32 = 0x07;
pub const SECTION_BM25_INDEX: u32 = 0x08;

// reserved additive optional sections. ids 0x09+ are reserved within frozen
// format v1 (see doc/research/master-plan.txt). all are EXCLUDED from
// content_hash (which covers the canonical six only), so adding any of them
// never invalidates a nest:// citation. claimed as named constants; NOT yet
// emitted or read until each feature ships its section codec and a manifest
// capability, at which point it joins OPTIONAL_SECTIONS.
pub const SECTION_EMBEDDINGS_FP: u32 = 0x09;
pub const SECTION_DICTIONARY: u32 = 0x0A;
pub const SECTION_DEDUP_MAP: u32 = 0x0B;
pub const SECTION_GRAPH_ADJACENCY: u32 = 0x0C;
pub const SECTION_CHUNK_SCALARS: u32 = 0x0D;
pub const SECTION_TOKENIZER_MODEL: u32 = 0x0E;
pub const SECTION_EDIT_JOURNAL: u32 = 0x0F;
pub const SECTION_REPRO_MANIFEST: u32 = 0x10;

// reconciled additive optional sections past 0x10 (see doc/plan/master-plan
// 02-format.txt). the four redesign pillars each independently proposed
// 0x11; this is the single disjoint map that resolves that collision in one
// pass, BEFORE any feature edits this file. ALL are EXCLUDED from
// content_hash, so adding any of them never invalidates a nest:// citation.
// claimed as named constants; NOT yet emitted or read until each feature
// ships its section codec and a manifest capability.
pub const SECTION_GRAPH_NODES: u32 = 0x11;
pub const SECTION_GRAPH_EDGE_PROPS: u32 = 0x12;
pub const SECTION_GRAPH_ENTITY_MAP: u32 = 0x13;
pub const SECTION_BLOB_REFS: u32 = 0x14;
pub const SECTION_SPACE_TABLE: u32 = 0x15;
// blob-relative span overlay: REPLACES the illegal chunks_original_spans
// v1->2 bump (spans is canonical + content-hashed + required). an excluded
// optional section keyed by chunk ordinal, so self_contained and catalog
// twins keep the SAME content_hash and old readers still open the file.
pub const SECTION_BLOB_SPAN_OVERLAY: u32 = 0x16;

// per-space vector bands. each non-text embedding space gets one fixed-
// stride 64-byte-aligned slab in 0x20-0x2F (NEVER zstd, scored by the
// existing simd kernels) and an optional matching fp rerank source in
// 0x30-0x3F. base + SPACE_BAND_LEN define each band; both excluded from
// content_hash. space[0]=text stays the canonical embeddings(0x04).
pub const SECTION_SPACE_EMBEDDINGS_BASE: u32 = 0x20;
pub const SECTION_SPACE_EMBEDDINGS_FP_BASE: u32 = 0x30;
pub const SPACE_BAND_LEN: u32 = 0x10;

/// lCanonical order for content_hash. Sorted alphabetically by name; this
/// order is fixed by spec so adding new section IDs cannot reshuffle the
/// hash. Keep this list and section IDs in sync.
pub const CANONICAL_SECTIONS: &[(u32, &str)] = &[
    (SECTION_CHUNK_IDS, "chunk_ids"),
    (SECTION_CHUNKS_CANONICAL, "chunks_canonical"),
    (SECTION_CHUNKS_ORIGINAL_SPANS, "chunks_original_spans"),
    (SECTION_EMBEDDINGS, "embeddings"),
    (SECTION_PROVENANCE, "provenance"),
    (SECTION_SEARCH_CONTRACT, "search_contract"),
];

/// lRequired sections for a v1 .nest file. A reader rejects any file
/// missing one of these with `MissingRequiredSection`.
pub const REQUIRED_SECTIONS: &[(u32, &str)] = CANONICAL_SECTIONS;

/// lOptional sections — present when their corresponding capability is
/// advertised in the manifest. They do NOT participate in content_hash
/// (which is over the canonical six only) so adding an optional section
/// to a corpus does not invalidate citations.
pub const OPTIONAL_SECTIONS: &[(u32, &str)] = &[
    (SECTION_HNSW_INDEX, "hnsw_index"),
    (SECTION_BM25_INDEX, "bm25_index"),
];

pub fn section_name(id: u32) -> Option<&'static str> {
    CANONICAL_SECTIONS
        .iter()
        .chain(OPTIONAL_SECTIONS.iter())
        .find(|(sid, _)| *sid == id)
        .map(|(_, name)| *name)
}

/// lCommon prefix for all internal section payloads (12 bytes):
///   u32 version (LE)
///   u64 entry_count (LE)
pub const SECTION_PAYLOAD_PREFIX_SIZE: usize = 12;
pub const SECTION_PAYLOAD_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_128() {
        assert_eq!(std::mem::size_of::<NestHeader>(), 128);
    }

    #[test]
    fn section_entry_size_is_32() {
        assert_eq!(std::mem::size_of::<SectionEntry>(), 32);
    }

    #[test]
    fn footer_size_is_40() {
        assert_eq!(std::mem::size_of::<NestFooter>(), 40);
    }

    #[test]
    fn header_roundtrip_checksum() {
        let mut h = NestHeader::new(384, 100, 100, 1024, 128, 5, 288, 200);
        assert!(h.validate_checksum().is_ok());
        h.n_chunks = 99;
        assert!(h.validate_checksum().is_err());
    }

    #[test]
    fn canonical_sections_are_alphabetical_by_name() {
        let names: Vec<&str> = CANONICAL_SECTIONS.iter().map(|(_, n)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn section_name_lookup() {
        assert_eq!(section_name(SECTION_CHUNK_IDS), Some("chunk_ids"));
        assert_eq!(section_name(SECTION_EMBEDDINGS), Some("embeddings"));
        assert_eq!(section_name(0xFFFF), None);
    }

    #[test]
    fn reserved_additive_ids_are_in_range_and_distinct() {
        // reserved wire encodings occupy 4..=9, above the four implemented ids.
        let enc = [
            SECTION_ENCODING_INTPACK,
            SECTION_ENCODING_ZSTD_DICT,
            SECTION_ENCODING_FRONTCODE,
            SECTION_ENCODING_INT4,
            SECTION_ENCODING_RABITQ,
            SECTION_ENCODING_FSST,
        ];
        for (i, &e) in enc.iter().enumerate() {
            assert_eq!(e, 4 + i as u32);
            assert!(e > SECTION_ENCODING_INT8);
        }
        // reserved optional sections occupy 0x09..=0x10, above the implemented eight.
        let sec = [
            SECTION_EMBEDDINGS_FP,
            SECTION_DICTIONARY,
            SECTION_DEDUP_MAP,
            SECTION_GRAPH_ADJACENCY,
            SECTION_CHUNK_SCALARS,
            SECTION_TOKENIZER_MODEL,
            SECTION_EDIT_JOURNAL,
            SECTION_REPRO_MANIFEST,
        ];
        for (i, &s) in sec.iter().enumerate() {
            assert_eq!(s, 0x09 + i as u32);
            assert!(s > SECTION_BM25_INDEX);
        }
        // reconciled additive ids past 0x10 are contiguous 0x11..=0x16; the
        // per-space bands start at 0x20 and 0x30 with 16 ids each. the
        // exhaustive disjointness + content_hash-exclusion check lives in
        // tests/reserved_ids.rs.
        let recon = [
            SECTION_GRAPH_NODES,
            SECTION_GRAPH_EDGE_PROPS,
            SECTION_GRAPH_ENTITY_MAP,
            SECTION_BLOB_REFS,
            SECTION_SPACE_TABLE,
            SECTION_BLOB_SPAN_OVERLAY,
        ];
        for (i, &s) in recon.iter().enumerate() {
            assert_eq!(s, 0x11 + i as u32);
        }
        assert_eq!(SECTION_SPACE_EMBEDDINGS_BASE, 0x20);
        assert_eq!(SECTION_SPACE_EMBEDDINGS_FP_BASE, 0x30);
        assert_eq!(SPACE_BAND_LEN, 0x10);

        // reserved sections are not yet advertised as active optional sections,
        // so section_name does not resolve them until their feature ships.
        for &s in sec.iter().chain(recon.iter()) {
            assert!(section_name(s).is_none());
        }
    }
}
