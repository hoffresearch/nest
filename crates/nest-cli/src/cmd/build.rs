//! `nest build --spec <file>` — launcher for the declarative build (RFC-0
//! N13: the build IS a python frontend; this verb resolves the interpreter
//! and the `nest_forge.py` tool, streams its output, and propagates the
//! exit code). The heavy lifting (spec validation, media, embedding, emit)
//! lives in python where torch/ffmpeg are; migration of spec validation to
//! rust is a registered future path, not this verb's job.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command as ProcCommand;

/// lResolve python/tools/nest_forge.py: repo layout first, then the
/// installed data-dir/share layouts (same ladder as the embedder scripts).
fn forge_tool_path() -> PathBuf {
    let rel: PathBuf = ["python", "tools", "nest_forge.py"].iter().collect();
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd.clone());
        bases.push(cwd.join(".."));
    }
    if let Some(repo) = super::embed_gate::exe_repo_root() {
        bases.push(repo);
    }
    for base in bases {
        let c = base.join(&rel);
        if c.exists() {
            return c;
        }
    }
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
    for base in [data_home, exe_share].into_iter().flatten() {
        let c = base.join("nest").join("tools").join("nest_forge.py");
        if c.exists() {
            return c;
        }
    }
    rel
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    spec: PathBuf,
    sample: Option<usize>,
    models: Option<String>,
    out_dir: Option<PathBuf>,
    resume: bool,
    rebuild_only: bool,
    dry_run: bool,
    allow_heavy: bool,
) -> Result<()> {
    let tool = forge_tool_path();
    if !tool.exists() {
        anyhow::bail!(
            "nest_forge.py not found ({}); run from the repo or install the forge payload",
            tool.display()
        );
    }
    let interpreter = super::pyenv::resolve_interpreter();
    let mut cmd = ProcCommand::new(interpreter);
    cmd.arg(&tool).arg("--spec").arg(&spec);
    if let Some(n) = sample {
        cmd.arg("--sample").arg(n.to_string());
    }
    if let Some(m) = &models {
        cmd.arg("--models").arg(m);
    }
    if let Some(d) = &out_dir {
        cmd.arg("--out-dir").arg(d);
    }
    for (flag, on) in [
        ("--resume", resume),
        ("--rebuild-only", rebuild_only),
        ("--dry-run", dry_run),
        ("--allow-heavy", allow_heavy),
    ] {
        if on {
            cmd.arg(flag);
        }
    }
    // stream child stdout/stderr straight through; the build's progress and
    // typed errors are the python tool's contract, not re-parsed here.
    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn {}: {}", tool.display(), e))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
