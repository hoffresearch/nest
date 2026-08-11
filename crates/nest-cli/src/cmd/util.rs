//! Shared CLI helpers: pretty-printers and embedder discovery.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command as ProcCommand;

use nest_runtime::{MmapNestFile, SearchResult};

pub fn print_result(result: &nest_runtime::SearchResult) {
    println!("index_type:   {}", result.index_type);
    if !result.recall.is_nan() {
        println!("recall:       {}", result.recall);
    } else {
        println!("recall:       (not computed; rerank guarantees real cosine)");
    }
    println!("truncated:    {}", result.truncated);
    println!("k_requested:  {}", result.k_requested);
    println!("k_returned:   {}", result.k_returned);
    println!("query_time:   {:.3} ms", result.query_time_ms);
    println!("hits:");
    for (i, hit) in result.hits.iter().enumerate() {
        println!(
            "  [{:3}] chunk_id={} score={:.6} score_type={} source_uri={} \
             offset={}-{} model={} index_type={} reranked={} file_hash={} \
             content_hash={} citation_id={}",
            i + 1,
            hit.chunk_id,
            hit.score,
            hit.score_type,
            hit.source_uri,
            hit.offset_start,
            hit.offset_end,
            hit.embedding_model,
            hit.index_type,
            hit.reranked,
            hit.file_hash,
            hit.content_hash,
            hit.citation_id,
        );
    }
}

pub fn encoding_name(e: u32) -> &'static str {
    match e {
        nest_format::layout::SECTION_ENCODING_RAW => "raw",
        nest_format::layout::SECTION_ENCODING_ZSTD => "zstd",
        nest_format::layout::SECTION_ENCODING_FLOAT16 => "float16",
        nest_format::layout::SECTION_ENCODING_INT8 => "int8",
        nest_format::layout::SECTION_ENCODING_INT4 => "int4",
        nest_format::layout::SECTION_ENCODING_INTPACK => "intpack",
        _ => "unknown",
    }
}

/// lWalk up from CARGO_MANIFEST_DIR / current dir to find python/embed_query.py.
pub fn default_embedder_path() -> PathBuf {
    let candidates = [
        std::env::current_dir()
            .ok()
            .map(|p| p.join("python").join("embed_query.py")),
        std::env::current_dir()
            .ok()
            .map(|p| p.join("..").join("python").join("embed_query.py")),
    ];
    for c in candidates.into_iter().flatten() {
        if c.exists() {
            return c;
        }
    }
    PathBuf::from("python/embed_query.py")
}

/// lFind the OFFLINE potion embedder script (`python/forge/embed_query_potion.py`).
/// the flagship verbs (`ask`/`retrieve`) embed with the default static potion
/// table, NEVER sentence-transformers, so an offline corpus gets a cited answer
/// with no network. distinct from `default_embedder_path` (the legacy
/// sentence-transformers path that `search-text` keeps).
///
/// resolution order: the repo layout first (dev checkout), then the installed
/// data dir (`$XDG_DATA_HOME/nest/forge/` or `~/.local/share/nest/forge/`),
/// then `<exe>/../share/nest/forge/` (homebrew and tarball layouts), where
/// the one-liner installer / package tarballs place the embedder payload
/// next to the potion table (issue #75).
pub fn default_potion_embedder_path() -> PathBuf {
    let rel = PathBuf::from("python")
        .join("forge")
        .join("embed_query_potion.py");
    let data_home = std::env::var("XDG_DATA_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local").join("share"))
        });
    let exe_share = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .map(|bin| bin.join("..").join("share"));
    let candidates = [
        std::env::current_dir().ok().map(|p| p.join(&rel)),
        std::env::current_dir()
            .ok()
            .map(|p| p.join("..").join(&rel)),
        data_home.map(|d| d.join("nest").join("forge").join("embed_query_potion.py")),
        exe_share.map(|s| s.join("nest").join("forge").join("embed_query_potion.py")),
    ];
    for c in candidates.into_iter().flatten() {
        if c.exists() {
            return c;
        }
    }
    rel
}

/// lThe embedder's json contract (shared by `embed_query.py` and the offline
/// `embed_query_potion.py`): the compact `model_hash` is the source of truth
/// for the manifest gate; `fingerprint` is diagnostic only.
#[derive(serde::Deserialize)]
struct EmbedderOutput {
    model_hash: String,
    embedding_model: String,
    embedding_dim: usize,
    vector: Vec<f32>,
    #[serde(default)]
    fingerprint: serde_json::Value,
}

const PLACEHOLDER_MODEL_HASH: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// lEmbed `query` OFFLINE with the potion table, validate the embedder's
/// model_hash against the manifest exactly like `search_text.rs`, then route
/// by manifest capability (`declared_index_type` / has_ann / has_bm25 /
/// has_graph) and run search. the returned `SearchResult` carries the
/// additive `SearchExplain` so callers can disclose the rerank-source honesty
/// line. shared by `ask` and `retrieve` so the gate lives in one place.
///
/// `model_path` overrides the vendored potion table dir for fully-offline
/// operation. a model_hash mismatch is a typed error (never a wrong score).
pub fn embed_and_search(
    runtime: &MmapNestFile,
    query: &str,
    k: i32,
    candidates: Option<usize>,
    embedder: Option<PathBuf>,
    model_path: Option<PathBuf>,
) -> Result<SearchResult> {
    let info: serde_json::Value = serde_json::from_str(&runtime.inspect_json()?)?;
    let model = info["manifest"]["embedding_model"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest.embedding_model missing"))?
        .to_string();
    let declared_dim = info["manifest"]["embedding_dim"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("manifest.embedding_dim missing"))?
        as usize;
    let declared_model_hash = info["manifest"]["model_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest.model_hash missing"))?
        .to_string();

    let embedder = embedder.unwrap_or_else(default_potion_embedder_path);
    if !embedder.exists() {
        anyhow::bail!(
            "offline embedder script not found: {} (override with --embedder)",
            embedder.display()
        );
    }

    // the interpreter the offline embedder runs under: `NEST_PYTHON`, else the
    // repo's `.venv`, else `python3` (see cmd::pyenv::resolve_interpreter). the
    // embedder itself opens no socket regardless of interpreter.
    let interpreter = super::pyenv::resolve_interpreter();
    let mut cmd = ProcCommand::new(&interpreter);
    cmd.arg(&embedder);
    if let Some(p) = &model_path {
        cmd.arg("--model-path").arg(p);
    }
    cmd.arg(&model).arg(query);
    let out = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn embedder: {} ({})", e, embedder.display()))?;
    if !out.status.success() {
        anyhow::bail!(
            "embedder failed (status={}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let payload: EmbedderOutput = serde_json::from_slice(&out.stdout).map_err(|e| {
        anyhow::anyhow!(
            "invalid embedder output: {} (stdout={:?})",
            e,
            String::from_utf8_lossy(&out.stdout)
        )
    })?;

    // lLayer 1/2/3 gate, identical to search_text.rs: name, dim, model_hash.
    if payload.embedding_model != model {
        anyhow::bail!(
            "model name mismatch: manifest={}, embedder reports={}",
            model,
            payload.embedding_model
        );
    }
    if payload.embedding_dim != declared_dim || payload.vector.len() != declared_dim {
        anyhow::bail!(
            "dim mismatch: manifest={}, embedder dim={}, vector len={}",
            declared_dim,
            payload.embedding_dim,
            payload.vector.len()
        );
    }
    if declared_model_hash == PLACEHOLDER_MODEL_HASH {
        anyhow::bail!(
            "manifest carries the legacy placeholder model_hash. rebuild this \
             corpus with a real fingerprint."
        );
    }
    if payload.model_hash != declared_model_hash {
        anyhow::bail!(
            "model_hash mismatch: corpus was built with {}, embedder reports {}\n\
             fingerprint reported by embedder: {}",
            declared_model_hash,
            payload.model_hash,
            payload.fingerprint
        );
    }

    // lRoute by manifest capability, then run. every printed score is the
    // exact-cosine rerank value (the candidate paths all end in score_subset).
    let cand = candidates.unwrap_or(((k as usize) * 4).max(64));
    let result = match runtime.declared_index_type() {
        "hnsw" => runtime.search_ann(&payload.vector, k, cand)?,
        "hybrid" => runtime.search_hybrid(&payload.vector, query, k, cand)?,
        "graph" => runtime.search_graph(&payload.vector, k, 1, cand)?,
        _ => runtime.search(&payload.vector, k)?,
    };
    Ok(result)
}
