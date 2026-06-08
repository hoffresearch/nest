//! The honest-rerank contract test (phase 0, task #01).
//!
//! This is THE gate for the project's core honesty claim: every non-exact
//! search path is a candidate GENERATOR, and the `score` it returns is the
//! real cosine recomputed by `score_subset`, never a candidate-generator
//! proxy (an ANN distance, a BM25/rrf rank, a graph edge weight, a per-
//! space ann score). The assertion is byte-for-byte: a hit's score from a
//! non-exact path must equal, bit-for-bit, the exact-cosine score the same
//! vector gets from flat exact search; and the path must report
//! `recall = NaN` (it never claims a recall it did not measure).
//!
//! PARAMETERIZED OVER EVERY SEARCH ENTRY POINT. Today that is `ann` and
//! `hybrid`. When `graph`, `space`, or `cross` paths land they MUST be
//! added to `non_exact_paths` below; a new non-exact search verb that is
//! not covered here is a release-check failure by policy (master-plan
//! 03-roadmap, 04-risks-quickwins). The list is the gate.

use nest_format::ChunkInput;
use nest_format::manifest::Manifest;
use nest_format::writer::NestFileBuilder;
use nest_runtime::ann::{DEFAULT_EF_CONSTRUCTION, DEFAULT_M, HnswIndex};
use nest_runtime::bm25::Bm25Index;
use nest_runtime::{MmapNestFile, SearchResult};
use std::collections::HashMap;
use std::path::PathBuf;

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(
            seed.wrapping_mul(2862933555777941757)
                .wrapping_add(3037000493),
        )
    }
    fn next_f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))) as f32
    }
}

fn random_l2(n: usize, dim: usize, seed: u64) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    let mut v = vec![0.0f32; n * dim];
    for x in v.iter_mut() {
        *x = rng.next_f32() - 0.5;
    }
    for i in 0..n {
        let row = &mut v[i * dim..(i + 1) * dim];
        let norm = row
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt()
            .max(f32::EPSILON);
        for x in row.iter_mut() {
            *x /= norm;
        }
    }
    v
}

/// lBuild a .nest carrying an HNSW (0x07) and a BM25 (0x08) section over a
/// synthetic f32 corpus, so `search_ann` and `search_hybrid` exercise the
/// real candidate paths rather than falling back to exact.
fn build_indexed_corpus(path: &PathBuf, n: usize, dim: usize) -> Vec<f32> {
    let vectors = random_l2(n, dim, 0xC0FFEE);
    let texts: Vec<String> = (0..n)
        .map(|i| format!("doc {i} alpha beta term{} shared{}", i, i % 7))
        .collect();

    let manifest = Manifest {
        embedding_model: "demo".into(),
        embedding_dim: dim as u32,
        n_chunks: n as u64,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    };

    let hnsw = HnswIndex::build(
        vectors.clone(),
        n,
        dim,
        DEFAULT_M,
        DEFAULT_EF_CONSTRUCTION,
        42,
    );
    let bm25 = Bm25Index::build(&texts, 1.2, 0.75);

    let mut builder = NestFileBuilder::new(manifest);
    for i in 0..n {
        builder = builder.add_chunk(ChunkInput {
            canonical_text: texts[i].clone(),
            source_uri: "doc.txt".into(),
            byte_start: (i * 10) as u64,
            byte_end: ((i + 1) * 10) as u64,
            embedding: vectors[i * dim..(i + 1) * dim].to_vec(),
        });
    }
    builder = builder
        .hnsw_index(hnsw.to_bytes())
        .bm25_index(bm25.to_bytes())
        .hybrid();
    builder.write_to_path(path).unwrap();
    vectors
}

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    p
}

/// lA non-exact search path: its name and a thunk that runs it. The list
/// of these is the honesty gate; new verbs must join it.
type NonExactPath<'a> = (&'a str, Box<dyn Fn() -> SearchResult + 'a>);

#[test]
fn non_exact_paths_return_real_cosine_byte_for_byte_and_recall_is_nan() {
    let dim = 32;
    let n = 300;
    let path = tmp_path("rt_rerank_contract.nest");
    let _ = std::fs::remove_file(&path);
    build_indexed_corpus(&path, n, dim);

    let rt = MmapNestFile::open(&path).unwrap();
    assert!(rt.has_ann(), "fixture must carry an HNSW section");
    assert!(rt.has_bm25(), "fixture must carry a BM25 section");

    // lGround truth: exact flat search over ALL chunks gives, per chunk,
    // the real-cosine score. We compare every non-exact hit against this
    // map by raw bits.
    let q = random_l2(1, dim, 0xABCD);
    let exact = rt.search(&q, n as i32).unwrap();
    assert_eq!(
        exact.recall, 1.0,
        "exact path is the recall=1.0 ground truth"
    );
    let truth: HashMap<String, u32> = exact
        .hits
        .iter()
        .map(|h| (h.chunk_id.clone(), h.score.to_bits()))
        .collect();

    // lEvery non-exact search entry point, by name + how to invoke it.
    // ADD graph/space/cross here when they land. The list is the gate.
    let non_exact_paths: Vec<NonExactPath> = {
        let qa = q.clone();
        let qh = q.clone();
        let rt_a = &rt;
        let rt_h = &rt;
        vec![
            (
                "ann",
                Box::new(move || rt_a.search_ann(&qa, 10, 100).unwrap()),
            ),
            (
                "hybrid",
                Box::new(move || {
                    rt_h.search_hybrid(&qh, "alpha shared3 term12", 10, 100)
                        .unwrap()
                }),
            ),
        ]
    };

    for (name, run) in &non_exact_paths {
        let res = run();
        assert!(
            res.recall.is_nan(),
            "{name}: a candidate-generating path must report recall=NaN, got {}",
            res.recall
        );
        assert!(
            !res.hits.is_empty(),
            "{name}: expected hits from the indexed fixture"
        );
        for hit in &res.hits {
            assert_eq!(
                hit.score_type, "cosine",
                "{name}: score_type must be cosine"
            );
            assert!(
                hit.reranked,
                "{name}: a non-exact path must mark hits reranked"
            );
            let want = truth.get(&hit.chunk_id).copied().unwrap_or_else(|| {
                panic!(
                    "{name}: hit {} not found in exact ground truth",
                    hit.chunk_id
                )
            });
            assert_eq!(
                hit.score.to_bits(),
                want,
                "{name}: returned score for {} is not the exact-cosine rerank value \
                 (got {}, exact {})",
                hit.chunk_id,
                hit.score,
                f32::from_bits(want),
            );
        }
    }

    let _ = std::fs::remove_file(&path);
}
