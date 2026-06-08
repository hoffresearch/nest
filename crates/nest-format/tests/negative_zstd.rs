//! Negative paths for the zstd encoding (`encoding=1`):
//!
//! - Reader rejects unsupported encoding values (e.g. `99`) at parse time.
//! - Reader rejects encoding=1 paired with the embeddings section (we
//!   keep embeddings mmap-friendly, never zstd).
//! - When a section's `encoding` byte is flipped from `0` (raw) to `1`
//!   (zstd) without changing the payload, the file passes physical
//!   integrity at parse time (the section's bytes weren't modified, so
//!   the checksum still matches; we recompute the footer's file_hash
//!   to keep the file structurally valid). But any consumer of
//!   `decoded_section` then trips a `MalformedSectionPayload` because
//!   zstd decompression of raw bytes fails.
//! - When an encoding=1 section's compressed payload is truncated
//!   mid-frame, decompression bails. The truncation is hidden behind
//!   a fresh checksum + file_hash so the test isolates the
//!   decompression failure from a checksum failure.

use nest_format::layout::{
    NEST_FOOTER_SIZE, NEST_HEADER_SIZE, NEST_SECTION_ENTRY_SIZE, NestFooter,
    SECTION_CHUNKS_CANONICAL, SECTION_EMBEDDINGS, SECTION_ENCODING_RAW, SECTION_ENCODING_ZSTD,
    SectionEntry,
};
use nest_format::manifest::{Capabilities, Manifest};
use nest_format::writer::{NestFileBuilder, SectionEncoding};
use nest_format::{ChunkInput, NestError, NestView};

fn manifest(n: u64) -> Manifest {
    Manifest {
        format_version: 1,
        schema_version: 1,
        embedding_model: "demo".into(),
        embedding_dim: 4,
        n_chunks: n,
        dtype: "float32".into(),
        metric: "ip".into(),
        score_type: "cosine".into(),
        normalize: "l2".into(),
        index_type: "exact".into(),
        rerank_policy: "none".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        chunker_version: "demo-chunker/1".into(),
        capabilities: Capabilities {
            supports_exact: true,
            supports_reproducible_build: true,
            supports_ann: false,
            supports_bm25: false,
            supports_citations: true,
        },
        title: None,
        version: None,
        created: None,
        description: None,
        authors: None,
        license: None,
        capabilities_ext: None,
        extra: Default::default(),
    }
}

fn good_chunks() -> Vec<ChunkInput> {
    (0..3)
        .map(|i| ChunkInput {
            canonical_text: format!("paragraph number {} with some content", i),
            source_uri: "doc.txt".into(),
            byte_start: i * 100,
            byte_end: i * 100 + 30,
            embedding: {
                let mut v = vec![0.0; 4];
                v[(i % 4) as usize] = 1.0;
                v
            },
        })
        .collect()
}

fn build_raw() -> Vec<u8> {
    NestFileBuilder::new(manifest(3))
        .reproducible(true)
        .add_chunks(good_chunks())
        .build_bytes()
        .unwrap()
}

fn build_zstd() -> Vec<u8> {
    NestFileBuilder::new(manifest(3))
        .text_encoding(SectionEncoding::Zstd)
        .reproducible(true)
        .add_chunks(good_chunks())
        .build_bytes()
        .unwrap()
}

/// lLocate the section table entry for `section_id` and return its
/// (offset within the file, parsed entry).
fn find_entry(bytes: &[u8], section_id: u32) -> (usize, SectionEntry) {
    let view = NestView::from_bytes(bytes).unwrap();
    let table_off = view.header.section_table_offset as usize;
    for (i, e) in view.section_table.iter().enumerate() {
        if e.section_id == section_id {
            return (table_off + i * NEST_SECTION_ENTRY_SIZE, *e);
        }
    }
    panic!("section 0x{:02x} not found", section_id);
}

/// lRewrite the file's `file_hash` in the footer to match the (possibly
/// tampered) body. Used to isolate a logical bug from a structural
/// one — physical checksums still pass, the bug is in the encoding /
/// payload contract.
fn rewrite_file_hash(bytes: &mut [u8]) {
    let body_end = bytes.len() - NEST_FOOTER_SIZE;
    let new_hash = NestFooter::compute_file_hash(&bytes[..body_end]);
    // lFooter layout: u64 footer_size (0..8) | [u8; 32] file_hash (8..40).
    bytes[body_end + 8..body_end + 40].copy_from_slice(&new_hash);
}

#[test]
fn rejects_unsupported_encoding_value() {
    let mut bytes = build_raw();
    let (entry_off, _) = find_entry(&bytes, SECTION_CHUNKS_CANONICAL);
    // lSet encoding to 99 (unallocated).
    bytes[entry_off + 4..entry_off + 8].copy_from_slice(&99u32.to_le_bytes());
    rewrite_file_hash(&mut bytes);

    match NestView::from_bytes(&bytes) {
        Err(NestError::UnsupportedSectionEncoding {
            section_id,
            encoding,
        }) => {
            assert_eq!(section_id, SECTION_CHUNKS_CANONICAL);
            assert_eq!(encoding, 99);
        }
        other => panic!(
            "expected UnsupportedSectionEncoding(99); got {:?}",
            other.as_ref().err()
        ),
    }
}

#[test]
fn rejects_zstd_encoding_on_embeddings_section() {
    // lEmbeddings must stay raw / float16 / int8 — never zstd. Even
    // though zstd is otherwise valid, applying it to the embeddings
    // section breaks the SIMD-on-mmap contract.
    let mut bytes = build_raw();
    let (entry_off, _) = find_entry(&bytes, SECTION_EMBEDDINGS);
    bytes[entry_off + 4..entry_off + 8].copy_from_slice(&SECTION_ENCODING_ZSTD.to_le_bytes());
    rewrite_file_hash(&mut bytes);

    match NestView::from_bytes(&bytes) {
        Err(NestError::UnsupportedSectionEncoding {
            section_id,
            encoding,
        }) => {
            assert_eq!(section_id, SECTION_EMBEDDINGS);
            assert_eq!(encoding, SECTION_ENCODING_ZSTD);
        }
        other => panic!(
            "expected UnsupportedSectionEncoding for zstd on embeddings; got {:?}",
            other.as_ref().err()
        ),
    }
}

#[test]
fn encoding_mismatch_passes_parse_but_fails_decode() {
    // lBuild a raw file, tag chunks_canonical as zstd-encoded, and
    // recompute the footer file_hash so the physical checksums all
    // pass. Parse succeeds (payload bytes weren't touched). But
    // `decoded_section` tries to zstd-decompress raw bytes and trips
    // `MalformedSectionPayload`.
    let mut bytes = build_raw();
    let (entry_off, _) = find_entry(&bytes, SECTION_CHUNKS_CANONICAL);
    // lSanity: was raw.
    let enc_pre = u32::from_le_bytes(bytes[entry_off + 4..entry_off + 8].try_into().unwrap());
    assert_eq!(enc_pre, SECTION_ENCODING_RAW);
    bytes[entry_off + 4..entry_off + 8].copy_from_slice(&SECTION_ENCODING_ZSTD.to_le_bytes());
    rewrite_file_hash(&mut bytes);

    let view = NestView::from_bytes(&bytes).expect("parse should still succeed");
    match view.decoded_section(SECTION_CHUNKS_CANONICAL) {
        Err(NestError::MalformedSectionPayload {
            section_id,
            reason: _,
        }) => {
            assert_eq!(section_id, SECTION_CHUNKS_CANONICAL);
        }
        other => panic!(
            "expected MalformedSectionPayload; got {:?}",
            other.as_ref().err()
        ),
    }
}

#[test]
fn truncated_zstd_payload_fails_decompression() {
    // lBuild a real zstd file, then chop the trailing bytes off the
    // chunks_canonical section's payload. Reset the section's checksum
    // and the file_hash so all structural checks pass; the bug surfaces
    // at decode time.
    let mut bytes = build_zstd();
    let (entry_off, mut entry) = find_entry(&bytes, SECTION_CHUNKS_CANONICAL);
    let payload_off = entry.offset as usize;
    let payload_end = payload_off + entry.size as usize;
    // lDrop the last 8 bytes of the zstd frame.
    let new_size = (entry.size as usize).saturating_sub(8);
    assert!(new_size > 4, "section too small to truncate");

    // lZero out the dropped bytes (they're padding-zone now); this keeps
    // the file's logical layout but the section size shrinks.
    for b in &mut bytes[payload_off + new_size..payload_end] {
        *b = 0;
    }

    // lRecompute the section checksum over the truncated payload.
    entry.size = new_size as u64;
    entry.compute_checksum(&bytes[payload_off..payload_off + new_size]);
    // lWrite the new entry back: section_id u32 | encoding u32 |
    // offset u64 | size u64 | checksum [u8; 8].
    bytes[entry_off..entry_off + 4].copy_from_slice(&entry.section_id.to_le_bytes());
    bytes[entry_off + 4..entry_off + 8].copy_from_slice(&entry.encoding.to_le_bytes());
    bytes[entry_off + 8..entry_off + 16].copy_from_slice(&entry.offset.to_le_bytes());
    bytes[entry_off + 16..entry_off + 24].copy_from_slice(&entry.size.to_le_bytes());
    bytes[entry_off + 24..entry_off + 32].copy_from_slice(&entry.checksum);
    rewrite_file_hash(&mut bytes);

    let view = NestView::from_bytes(&bytes).expect("parse should still succeed");
    match view.decoded_section(SECTION_CHUNKS_CANONICAL) {
        Err(NestError::MalformedSectionPayload { section_id, .. }) => {
            assert_eq!(section_id, SECTION_CHUNKS_CANONICAL);
        }
        other => panic!(
            "expected MalformedSectionPayload from truncated zstd; got {:?}",
            other.as_ref().err()
        ),
    }
}

#[test]
fn corrupted_zstd_magic_fails_decompression() {
    // lBuild a valid zstd file, then flip the first 4 bytes of the
    // chunks_canonical payload (zstd frame magic = 0x28 0xB5 0x2F 0xFD)
    // to garbage. Recompute checksum + file_hash. Parse passes,
    // decompression fails.
    let mut bytes = build_zstd();
    let (entry_off, mut entry) = find_entry(&bytes, SECTION_CHUNKS_CANONICAL);
    let payload_off = entry.offset as usize;
    bytes[payload_off..payload_off + 4].copy_from_slice(&[0xFF, 0xEE, 0xDD, 0xCC]);

    entry.compute_checksum(&bytes[payload_off..payload_off + entry.size as usize]);
    bytes[entry_off + 24..entry_off + 32].copy_from_slice(&entry.checksum);
    rewrite_file_hash(&mut bytes);

    let view = NestView::from_bytes(&bytes).expect("parse should still succeed");
    let res = view.decoded_section(SECTION_CHUNKS_CANONICAL);
    assert!(
        matches!(res, Err(NestError::MalformedSectionPayload { .. })),
        "expected MalformedSectionPayload from bad zstd magic; got {:?}",
        res.as_ref().err()
    );
}

#[test]
fn baseline_zstd_decodes_normally() {
    // lSanity: a properly-built zstd file decodes without errors.
    // lGuards against false-positive negatives in the tests above.
    let bytes = build_zstd();
    let view = NestView::from_bytes(&bytes).unwrap();
    let decoded = view.decoded_section(SECTION_CHUNKS_CANONICAL).unwrap();
    assert!(!decoded.is_empty());
    let _ = decoded; // owned Cow when zstd; just verify it decoded.
    let _ = NEST_HEADER_SIZE; // silence unused import warning
}
