#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failing unwrap is a failing test"
)]
use nest_format::ChunkInput;
use nest_format::manifest::Manifest;
use nest_format::reader::NestView;
use nest_format::writer::NestFileBuilder;

fn main() {
    let m = Manifest {
        embedding_model: "demo".into(),
        embedding_dim: 4,
        n_chunks: 1,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    };
    let bytes = NestFileBuilder::new(m)
        .add_chunk(ChunkInput {
            canonical_text: "hi".into(),
            source_uri: "doc.txt".into(),
            byte_start: 0,
            byte_end: 2,
            embedding: vec![1.0, 0.0, 0.0, 0.0],
        })
        .build_bytes()
        .unwrap();

    let path = "crates/nest-format/tests/fixtures/golden_v1_minimal.nest";
    std::fs::write(path, &bytes).unwrap();

    let view = NestView::from_bytes(&bytes).unwrap();
    println!("GOLDEN_LEN = {}", bytes.len());
    println!("GOLDEN_FILE_HASH = {}", view.file_hash_hex());
    println!("GOLDEN_CONTENT_HASH = {}", view.content_hash_hex().unwrap());
    let ids = nest_format::sections::decode_chunk_ids(
        view.get_section_data(nest_format::layout::SECTION_CHUNK_IDS)
            .unwrap(),
        1,
    )
    .unwrap();
    println!("GOLDEN_CHUNK_ID = {}", ids[0]);
}
