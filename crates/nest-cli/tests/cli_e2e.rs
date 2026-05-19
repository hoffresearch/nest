use std::path::PathBuf;
use std::process::Command;

use nest_format::ChunkInput;
use nest_format::manifest::Manifest;
use nest_format::writer::NestFileBuilder;

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    p
}

fn build_test_file(path: &PathBuf, dim: usize, n: usize) {
    let mut builder = NestFileBuilder::new(Manifest {
        embedding_model: "demo".into(),
        embedding_dim: dim as u32,
        n_chunks: n as u64,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    });
    for i in 0..n {
        let mut emb = vec![0.0f32; dim];
        emb[i % dim] = 1.0;
        builder = builder.add_chunk(ChunkInput {
            canonical_text: format!("text_{}", i),
            source_uri: "doc.txt".into(),
            byte_start: (i * 10) as u64,
            byte_end: ((i + 1) * 10) as u64,
            embedding: emb,
        });
    }
    builder.write_to_path(path).unwrap();
}

#[test]
fn cli_validate_ok() {
    let path = tmp_path("cli_validate.nest");
    let _ = std::fs::remove_file(&path);
    build_test_file(&path, 8, 10);

    let bin = env!("CARGO_BIN_EXE_nest");
    let out = Command::new(bin)
        .args(["validate", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("valid .nest v1 file"));
    assert!(stdout.contains("Required sections:"));
    assert!(stdout.contains("File hash:"));
    assert!(stdout.contains("Content hash:"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_search_ok() {
    let path = tmp_path("cli_search.nest");
    let _ = std::fs::remove_file(&path);
    build_test_file(&path, 4, 5);

    let bin = env!("CARGO_BIN_EXE_nest");

    // stats
    let out = Command::new(bin)
        .args(["stats", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("chunks:       5"));
    assert!(stdout.contains("dtype:        float32"));
    assert!(stdout.contains("metric:       ip"));

    // search aligned to first axis returns chunk that maps to axis 0
    let query = "[1.0, 0.0, 0.0, 0.0]";
    let out = Command::new(bin)
        .args(["search", path.to_str().unwrap(), query, "-k", "1"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("index_type:"));
    assert!(stdout.contains("recall:"));
    assert!(stdout.contains("score=1.000000"));
    assert!(stdout.contains("citation_id=nest://sha256:"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_cite_resolves_citation() {
    let path = tmp_path("cli_cite.nest");
    let _ = std::fs::remove_file(&path);
    build_test_file(&path, 4, 2);

    let bin = env!("CARGO_BIN_EXE_nest");

    // lFirst, fetch a real citation_id by running search.
    let q = "[1.0, 0.0, 0.0, 0.0]";
    let out = Command::new(bin)
        .args(["search", path.to_str().unwrap(), q, "-k", "1"])
        .output()
        .unwrap();
    assert!(out.status.success(), "search failed");
    let stdout = String::from_utf8_lossy(&out.stdout);

    let cit_token = stdout
        .split_whitespace()
        .find(|t| t.starts_with("citation_id=nest://"))
        .expect("citation_id present in search output");
    let citation = cit_token
        .strip_prefix("citation_id=")
        .expect("citation_id= prefix");

    let out = Command::new(bin)
        .args(["cite", path.to_str().unwrap(), citation])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cite failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("source_uri:   doc.txt"));
    assert!(stdout.contains("byte_start:"));
    assert!(stdout.contains("byte_end:"));
    assert!(stdout.contains("text_"));

    // lMismatched content_hash → cite must fail loudly.
    let bogus = format!("nest://sha256:{}/sha256:{}", "0".repeat(64), "0".repeat(64));
    let out = Command::new(bin)
        .args(["cite", path.to_str().unwrap(), &bogus])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "cite should reject content_hash mismatch"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_inspect_shows_section_names() {
    let path = tmp_path("cli_inspect.nest");
    let _ = std::fs::remove_file(&path);
    build_test_file(&path, 4, 2);

    let bin = env!("CARGO_BIN_EXE_nest");
    let out = Command::new(bin)
        .args(["inspect", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("chunk_ids"));
    assert!(stdout.contains("chunks_canonical"));
    assert!(stdout.contains("chunks_original_spans"));
    assert!(stdout.contains("embeddings"));
    assert!(stdout.contains("provenance"));
    assert!(stdout.contains("search_contract"));

    let _ = std::fs::remove_file(&path);
}
