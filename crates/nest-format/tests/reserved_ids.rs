//! Reconciled additive section-id reservation (phase 0, task #02).
//!
//! The four redesign pillars (graph, catalog, multimodal, lens) each
//! independently proposed 0x11. This asserts the single reconciled map is
//! disjoint and content_hash-safe, claimed in ONE pass so no feature
//! branch edits layout/mod.rs into a collision. Every reserved id past the
//! implemented 0x01..=0x08 must be: distinct from every other reserved id,
//! excluded from CANONICAL_SECTIONS (so it never enters content_hash and
//! never invalidates a nest:// citation), and unresolved by section_name
//! until its feature ships.

use nest_format::layout::{
    CANONICAL_SECTIONS, SECTION_BLOB_REFS, SECTION_BLOB_SPAN_OVERLAY, SECTION_BM25_INDEX,
    SECTION_CHUNK_IDS, SECTION_CHUNK_SCALARS, SECTION_CHUNKS_CANONICAL,
    SECTION_CHUNKS_ORIGINAL_SPANS, SECTION_DEDUP_MAP, SECTION_DICTIONARY, SECTION_EDIT_JOURNAL,
    SECTION_EMBEDDINGS, SECTION_EMBEDDINGS_FP, SECTION_GRAPH_ADJACENCY, SECTION_GRAPH_EDGE_PROPS,
    SECTION_GRAPH_ENTITY_MAP, SECTION_GRAPH_NODES, SECTION_HNSW_INDEX, SECTION_PROVENANCE,
    SECTION_REPRO_MANIFEST, SECTION_SEARCH_CONTRACT, SECTION_SPACE_EMBEDDINGS_BASE,
    SECTION_SPACE_EMBEDDINGS_FP_BASE, SECTION_SPACE_TABLE, SECTION_TOKENIZER_MODEL, SPACE_BAND_LEN,
    section_name,
};

fn implemented() -> Vec<u32> {
    vec![
        SECTION_CHUNK_IDS,
        SECTION_CHUNKS_CANONICAL,
        SECTION_CHUNKS_ORIGINAL_SPANS,
        SECTION_EMBEDDINGS,
        SECTION_PROVENANCE,
        SECTION_SEARCH_CONTRACT,
        SECTION_HNSW_INDEX,
        SECTION_BM25_INDEX,
    ]
}

/// lReserved scalar ids (0x09..=0x16): not yet emitted/read.
fn reserved_scalars() -> Vec<u32> {
    vec![
        SECTION_EMBEDDINGS_FP,
        SECTION_DICTIONARY,
        SECTION_DEDUP_MAP,
        SECTION_GRAPH_ADJACENCY,
        SECTION_CHUNK_SCALARS,
        SECTION_TOKENIZER_MODEL,
        SECTION_EDIT_JOURNAL,
        SECTION_REPRO_MANIFEST,
        SECTION_GRAPH_NODES,
        SECTION_GRAPH_EDGE_PROPS,
        SECTION_GRAPH_ENTITY_MAP,
        SECTION_BLOB_REFS,
        SECTION_SPACE_TABLE,
        SECTION_BLOB_SPAN_OVERLAY,
    ]
}

fn space_band() -> Vec<u32> {
    (SECTION_SPACE_EMBEDDINGS_BASE..SECTION_SPACE_EMBEDDINGS_BASE + SPACE_BAND_LEN).collect()
}

fn space_fp_band() -> Vec<u32> {
    (SECTION_SPACE_EMBEDDINGS_FP_BASE..SECTION_SPACE_EMBEDDINGS_FP_BASE + SPACE_BAND_LEN).collect()
}

#[test]
fn all_reserved_bands_are_disjoint() {
    let mut all: Vec<u32> = Vec::new();
    all.extend(implemented());
    all.extend(reserved_scalars());
    all.extend(space_band());
    all.extend(space_fp_band());

    let mut sorted = all.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        all.len(),
        "every section id (implemented + reserved + per-space bands) must be disjoint"
    );

    // lThe two per-space bands stay inside their documented ranges.
    assert!(space_band().iter().all(|&x| (0x20..=0x2F).contains(&x)));
    assert!(space_fp_band().iter().all(|&x| (0x30..=0x3F).contains(&x)));
}

#[test]
fn reserved_ids_are_excluded_from_content_hash_and_unresolved() {
    let canonical: Vec<u32> = CANONICAL_SECTIONS.iter().map(|(id, _)| *id).collect();
    let mut reserved = reserved_scalars();
    reserved.extend(space_band());
    reserved.extend(space_fp_band());

    for s in reserved {
        assert!(
            !canonical.contains(&s),
            "reserved id {s:#x} must be excluded from content_hash (canonical six only)"
        );
        assert!(
            section_name(s).is_none(),
            "reserved id {s:#x} must not resolve via section_name until its feature ships"
        );
    }
}
