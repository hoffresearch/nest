//! `nest stats <file>` — size, dim, dtype, hashes, per-section sizes,
//! SIMD backend.

use anyhow::Result;
use std::path::PathBuf;

use super::util::encoding_name;

pub fn run(file: PathBuf) -> Result<()> {
    let data = std::fs::read(&file)?;
    let view = nest_format::NestView::from_bytes(&data)?;
    let size = std::fs::metadata(&file)?.len();
    println!("file:         {}", file.display());
    println!("size:         {} bytes ({:.2} MB)", size, size as f64 / 1e6);
    println!("chunks:       {}", view.header.n_chunks);
    println!("embeddings:   {}", view.header.n_embeddings);
    println!("dim:          {}", view.header.embedding_dim);
    // lmatryoshka disclosure: when the file was built with prefix truncation,
    // surface the effective prefix dim and the full source dim so the
    // size/recall tradeoff is visible. content_hash is over the truncated
    // embeddings, so a citation is tied to this mrl_dim.
    if let (Some(mrl), Some(full)) = (view.manifest.mrl_dim, view.manifest.full_dim) {
        println!("mrl_dim:      {} (prefix of {})", mrl, full);
        println!("full_dim:     {}", full);
    }
    println!("dtype:        {}", view.manifest.dtype);
    println!("metric:       {}", view.manifest.metric);
    println!("score_type:   {}", view.manifest.score_type);
    println!("normalize:    {}", view.manifest.normalize);
    println!("index_type:   {}", view.manifest.index_type);
    println!("rerank:       {}", view.manifest.rerank_policy);
    println!("model:        {}", view.manifest.embedding_model);
    println!("model_hash:   {}", view.manifest.model_hash);
    println!("chunker:      {}", view.manifest.chunker_version);
    println!("supports_ann: {}", view.manifest.capabilities.supports_ann);
    println!("supports_bm25:{}", view.manifest.capabilities.supports_bm25);
    println!(
        "simd_backend: {}",
        nest_runtime::simd::detect_backend().name()
    );
    println!("sections:     {}", view.section_table.len());
    for entry in &view.section_table {
        let name = nest_format::layout::section_name(entry.section_id).unwrap_or("unknown");
        let enc = encoding_name(entry.encoding);
        println!(
            "  0x{:02x} {:<24} encoding={:<8} {} bytes",
            entry.section_id, name, enc, entry.size
        );
    }
    println!("file_hash:    {}", view.file_hash_hex());
    println!("content_hash: {}", view.content_hash_hex()?);
    Ok(())
}
