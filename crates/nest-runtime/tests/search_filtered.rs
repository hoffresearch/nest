//! search_filtered (0x17 meta_index) runtime integration coverage.
//!
//! Closes the gap the independent review flagged: the build()->open()->
//! search_filtered() path had zero integration tests. Verifies invariant (3)
//! (the filter restricts the exact cosine to the (field,value) subset; a missing
//! field/value or a no-meta_index file returns an honest EMPTY result, never a
//! silent whole-corpus fallback) and invariant (1) (no panic on the empty /
//! k>>subset paths).

use nest_format::ChunkInput;
use nest_format::manifest::Manifest;
use nest_format::writer::NestFileBuilder;
use nest_runtime::MmapNestFile;
use nest_runtime::meta::MetaIndex;
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    p
}

fn manifest(dim: u32, n: u64) -> Manifest {
    Manifest {
        embedding_model: "demo".into(),
        embedding_dim: dim,
        n_chunks: n,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    }
}

/// 4 axis-aligned chunks; optional field "g" labels them [a, b, a, b].
fn build_file(path: &PathBuf, with_meta: bool) {
    let (dim, n) = (4usize, 4usize);
    let mut builder = NestFileBuilder::new(manifest(dim as u32, n as u64));
    for i in 0..n {
        let mut emb = vec![0.0f32; dim];
        emb[i % dim] = 1.0;
        builder = builder.add_chunk(ChunkInput {
            canonical_text: format!("text_{i}"),
            source_uri: "doc.txt".into(),
            byte_start: (i * 10) as u64,
            byte_end: ((i + 1) * 10) as u64,
            embedding: emb,
        });
    }
    if with_meta {
        let cols = vec![(
            "g".to_string(),
            vec![Some("a".into()), Some("b".into()), Some("a".into()), Some("b".into())],
        )];
        builder = builder.meta_index(MetaIndex::build(&cols).to_bytes());
    }
    builder.write_to_path(path).unwrap();
}

#[test]
fn filter_restricts_to_subset() {
    let path = tmp_path("rt_filtered.nest");
    let _ = std::fs::remove_file(&path);
    build_file(&path, true);
    let rt = MmapNestFile::open(&path).unwrap();
    assert!(rt.has_meta_index());
    assert_eq!(rt.meta_index_fields(), vec!["g".to_string()]);

    // query == axis 0 (chunk 0). g=a is chunks {0, 2}; offsets 0 and 20.
    let q = vec![1.0f32, 0.0, 0.0, 0.0];
    let res = rt.search_filtered(&q, "g", "a", 10).unwrap();
    assert_eq!(res.index_type, "filtered");
    assert_eq!(res.recall, 1.0); // subset non-empty -> exact within filter
    assert!(res.hits.len() <= 2, "only the 2 g=a chunks are candidates");
    // membership: every hit is chunk 0 or 2 (byte_start in {0,20}, i.e. %20==0).
    for h in &res.hits {
        assert_eq!(h.offset_start % 20, 0, "hit escaped the g=a subset");
    }
    // chunk 0 is the exact match, must rank first at cosine ~1.0.
    assert_eq!(res.hits[0].offset_start, 0);
    assert!((res.hits[0].score - 1.0).abs() < 1e-6);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_value_field_and_no_index_are_honest_empty() {
    let with = tmp_path("rt_filtered_present.nest");
    let without = tmp_path("rt_filtered_absent.nest");
    let _ = std::fs::remove_file(&with);
    let _ = std::fs::remove_file(&without);
    build_file(&with, true);
    build_file(&without, false);
    let q = vec![1.0f32, 0.0, 0.0, 0.0];

    let rt = MmapNestFile::open(&with).unwrap();
    // absent value -> empty, NaN recall (recall of nothing), no whole-corpus fallback.
    let v = rt.search_filtered(&q, "g", "zzz", 10).unwrap();
    assert_eq!(v.hits.len(), 0);
    assert!(v.recall.is_nan());
    // absent field -> empty.
    assert_eq!(rt.search_filtered(&q, "nope", "a", 10).unwrap().hits.len(), 0);
    // k far exceeding the subset must not panic.
    assert!(rt.search_filtered(&q, "g", "a", 1000).unwrap().hits.len() <= 2);

    // a file with NO meta_index -> empty, NaN recall, never a corpus fallback.
    let rt2 = MmapNestFile::open(&without).unwrap();
    assert!(!rt2.has_meta_index());
    let e = rt2.search_filtered(&q, "g", "a", 10).unwrap();
    assert_eq!(e.hits.len(), 0);
    assert!(e.recall.is_nan());

    let _ = std::fs::remove_file(&with);
    let _ = std::fs::remove_file(&without);
}
