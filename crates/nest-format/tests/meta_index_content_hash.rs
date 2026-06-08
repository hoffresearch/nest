//! meta_index (0x17) content_hash exclusion, end-to-end.
//!
//! A fixed corpus WITH vs WITHOUT the 0x17 meta_index section must have an
//! IDENTICAL content_hash (so a nest:// citation stays stable) but a differing
//! file_hash. This is the regression the structural reserved_ids check cannot
//! give: it would catch a future refactor of content_hash_hex / CANONICAL_SECTIONS
//! that accidentally folded 0x17 into the hash. Mirrors the 0x0C graph test.

use nest_format::manifest::Manifest;
use nest_format::writer::NestFileBuilder;
use nest_format::{ChunkInput, NestView};

fn build_corpus(path: &std::path::Path, with_meta: bool) {
    let n = 6usize;
    let dim = 4usize;
    let manifest = Manifest {
        embedding_model: "demo".into(),
        embedding_dim: dim as u32,
        n_chunks: n as u64,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    };
    let mut builder = NestFileBuilder::new(manifest).reproducible(true);
    for i in 0..n {
        let mut emb = vec![0.0f32; dim];
        emb[i % dim] = 1.0;
        builder = builder.add_chunk(ChunkInput {
            canonical_text: format!("chunk number {i} text"),
            source_uri: "doc.txt".into(),
            byte_start: (i * 10) as u64,
            byte_end: ((i + 1) * 10) as u64,
            embedding: emb,
        });
    }
    if with_meta {
        // an opaque 0x17 payload: content_hash excludes the section by id, so its
        // exact bytes are irrelevant here (header shape: version=1, n_fields=0).
        builder = builder.meta_index(vec![1u8, 0, 0, 0, 0, 0, 0, 0]);
    }
    builder.write_to_path(path).unwrap();
}

#[test]
fn meta_index_section_does_not_change_content_hash() {
    let mut a = std::env::temp_dir();
    a.push("meta_ch_without.nest");
    let mut b = std::env::temp_dir();
    b.push("meta_ch_with.nest");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    build_corpus(&a, false);
    build_corpus(&b, true);

    let da = std::fs::read(&a).unwrap();
    let db = std::fs::read(&b).unwrap();
    let va = NestView::from_bytes(&da).unwrap();
    let vb = NestView::from_bytes(&db).unwrap();

    assert_eq!(
        va.content_hash_hex().unwrap(),
        vb.content_hash_hex().unwrap(),
        "the 0x17 meta_index section must NOT change content_hash (citations stable)"
    );
    // the meta build legitimately moves file_hash (the bytes differ).
    assert_ne!(va.file_hash_hex(), vb.file_hash_hex());
    // and the with-meta file genuinely carries the section.
    assert!(vb.entry(0x17).is_ok(), "with-meta file must carry 0x17");
    assert!(va.entry(0x17).is_err(), "without-meta file must not");

    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}
