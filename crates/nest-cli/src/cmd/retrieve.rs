//! `nest retrieve <file> "<query>"` - the agent-shaped flagship surface.
//! emits cited spans as a json/jsonl answer-pack where each hit's `score`
//! IS the exact-cosine rerank value (guarded by rerank_contract.rs), not a
//! candidate-generator proxy. embeds the query OFFLINE with the default
//! potion table (never sentence-transformers) and validates model_hash
//! against the manifest, the same gate `search-text`/`ask` use.
//!
//! answer-pack shape per hit: chunk_id, score, score_type=cosine,
//! source_uri, offset_start/offset_end, citation_id
//! (nest://content_hash/chunk_id), text (stored canonical, TIER-1),
//! file_hash, content_hash, plus the rerank_source disclosure.
//!
//! cite stays TIER-1 ONLY: `text` is the stored canonical text + verifying
//! hashes, exactly what `cite` returns today. this verb NEVER claims
//! original-byte reopen (that is net-new tier-2 catalog, post-gate). the
//! citation_id round-trips through `nest cite`.

use anyhow::Result;
use std::path::PathBuf;

use super::embed_gate::embed_and_search;

/// output format for the answer-pack: `jsonl` (one json object per line, the
/// agent-native streaming shape, default) or `json` (a single pretty array).
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Format {
    Jsonl,
    Json,
}

/// Decode the stored canonical text for every chunk, returned as
/// `(chunk_id, text)` pairs in file order. TIER-1: these are the stored
/// canonical bytes, the same text `nest cite` returns, NEVER the original
/// source bytes. shared with `ask`.
pub fn canonical_texts(file: &PathBuf) -> Result<Vec<(String, String)>> {
    let data = std::fs::read(file)?;
    let view = nest_format::NestView::from_bytes(&data)?;
    let n = view.header.n_chunks as usize;
    let ids = nest_format::sections::decode_chunk_ids(
        &view.decoded_section(nest_format::layout::SECTION_CHUNK_IDS)?,
        n,
    )?;
    let texts = nest_format::sections::decode_chunks_canonical(
        &view.decoded_section(nest_format::layout::SECTION_CHUNKS_CANONICAL)?,
        n,
    )?;
    Ok(ids.into_iter().zip(texts).collect())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: PathBuf,
    query: String,
    k: i32,
    format: Format,
    embedder: Option<PathBuf>,
    candidates: Option<usize>,
    model_path: Option<PathBuf>,
) -> Result<()> {
    let runtime = nest_runtime::MmapNestFile::open(&file)?;
    let result = embed_and_search(&runtime, &query, k, candidates, embedder, model_path)?;

    let texts = canonical_texts(&file)?;
    let by_id: std::collections::HashMap<&str, &str> = texts
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();

    // lthe rerank-source disclosure rides on every hit so an agent consuming
    // jsonl knows the score precision without a second call (honesty by
    // default). it is identical for all hits in one response.
    let rerank_source = result.explain.rerank_source.as_str();

    let packs: Vec<serde_json::Value> = result
        .hits
        .iter()
        .map(|hit| {
            let text = by_id.get(hit.chunk_id.as_str()).copied().unwrap_or("");
            serde_json::json!({
                "chunk_id": hit.chunk_id,
                // lthe load-bearing claim: score IS the exact-cosine rerank value.
                "score": hit.score,
                "score_type": hit.score_type,
                "source_uri": hit.source_uri,
                "offset_start": hit.offset_start,
                "offset_end": hit.offset_end,
                "citation_id": hit.citation_id,
                // ltier-1 stored canonical text (NOT original-byte reopen).
                "text": text,
                "file_hash": hit.file_hash,
                "content_hash": hit.content_hash,
                "rerank_source": rerank_source,
            })
        })
        .collect();

    match format {
        Format::Jsonl => {
            for p in &packs {
                println!("{}", serde_json::to_string(p)?);
            }
        }
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&packs)?);
        }
    }
    Ok(())
}
