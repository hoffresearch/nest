//! `nest search-text <file> "query" -k K` — embed the query via
//! `python/embed_query.py`, validate model_hash against the manifest,
//! route to the declared `index_type`.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command as ProcCommand;

use super::util::{default_embedder_path, print_result};

/// lOutput schema produced by `python/embed_query.py`.
#[derive(serde::Deserialize)]
struct EmbedderOutput {
    model_hash: String,
    embedding_model: String,
    embedding_dim: usize,
    vector: Vec<f32>,
    /// lFull structured fingerprint — included here for diagnostics on
    /// mismatch but not validated field-by-field (the compact
    /// `model_hash` is the source of truth).
    #[serde(default)]
    fingerprint: serde_json::Value,
}

/// lLegacy zero placeholder; pre-Phase-3 corpora may have this. Caller
/// can opt out of strict validation via `--skip-model-hash-check` to
/// search them.
const PLACEHOLDER_MODEL_HASH: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: PathBuf,
    query: String,
    k: i32,
    embedder: Option<PathBuf>,
    candidates: Option<usize>,
    model_path: Option<PathBuf>,
    skip_model_hash_check: bool,
) -> Result<()> {
    let runtime = nest_runtime::MmapNestFile::open(&file)?;
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

    let embedder = embedder.unwrap_or_else(default_embedder_path);
    if !embedder.exists() {
        anyhow::bail!(
            "embedder script not found: {} (override with --embedder)",
            embedder.display()
        );
    }

    eprintln!(
        "[nest] embedding query with {} via {}{}",
        model,
        embedder.display(),
        match &model_path {
            Some(p) => format!(" (--model-path {})", p.display()),
            None => String::new(),
        }
    );
    let mut cmd = ProcCommand::new("python3");
    cmd.arg(&embedder);
    if let Some(p) = &model_path {
        cmd.arg("--model-path").arg(p);
    }
    cmd.arg(&model).arg(&query);
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

    // lLayer 1: name match (cheap; catches obvious mistakes).
    if payload.embedding_model != model {
        anyhow::bail!(
            "model name mismatch: manifest={}, embedder reports={}",
            model,
            payload.embedding_model
        );
    }
    // lLayer 2: dim match (cheap; catches dim collisions).
    if payload.embedding_dim != declared_dim || payload.vector.len() != declared_dim {
        anyhow::bail!(
            "dim mismatch: manifest={}, embedder dim={}, vector len={}",
            declared_dim,
            payload.embedding_dim,
            payload.vector.len()
        );
    }
    // lLayer 3: model_hash match (the strict check; catches "same name,
    // same dim, different snapshot" silent failures).
    if !skip_model_hash_check {
        if declared_model_hash == PLACEHOLDER_MODEL_HASH {
            anyhow::bail!(
                "manifest carries the legacy placeholder model_hash ({}). Rebuild \
                 this corpus with a real fingerprint, or pass --skip-model-hash-check \
                 to proceed at your own risk.",
                PLACEHOLDER_MODEL_HASH
            );
        }
        if payload.model_hash != declared_model_hash {
            anyhow::bail!(
                "model_hash mismatch: corpus was built with {}, embedder reports {}\n\
                 fingerprint reported by embedder: {}\n\
                 hint: --model-path PATH to point at the exact snapshot, or rebuild \
                 the corpus with the model you intend to use.",
                declared_model_hash,
                payload.model_hash,
                payload.fingerprint
            );
        }
    }

    let cand = candidates.unwrap_or(((k as usize) * 4).max(64));
    let result = match runtime.declared_index_type() {
        "hnsw" => runtime.search_ann(&payload.vector, k, cand)?,
        "hybrid" => runtime.search_hybrid(&payload.vector, &query, k, cand)?,
        _ => runtime.search(&payload.vector, k)?,
    };
    print_result(&result);
    Ok(())
}
