//! The managed python env: `<data root>/urna/venv` with numpy and
//! tokenizers, the two packages the potion embedder imports. uv builds it
//! when it is on PATH (fast, and it can fetch a python when the machine has
//! none); otherwise the stock `python3 -m venv` + pip. every line either
//! tool prints is streamed to `log`, so the installer shows real output.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::cmd::paths;

/// the same floors as `packaging/pyproject.toml` (`urna[embed]`).
pub const DEPS: [&str; 2] = ["numpy>=1.26", "tokenizers>=0.20"];

#[derive(Clone, Debug)]
pub enum Tool {
    Uv(PathBuf),
    Pip(String),
}

impl Tool {
    pub fn pick(uv: Option<&Path>, base_python: Option<&str>) -> Option<Tool> {
        uv.map(|p| Tool::Uv(p.to_path_buf()))
            .or_else(|| base_python.map(|p| Tool::Pip(p.to_string())))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tool::Uv(_) => "uv",
            Tool::Pip(_) => "pip",
        }
    }
}

/// Runs `cmd`, forwarding stdout and stderr line by line to `log`.
pub fn stream(mut cmd: Command, log: &mut dyn FnMut(String)) -> Result<()> {
    let shown = format!("{cmd:?}").replace('"', "");
    log(format!("$ {shown}"));
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {shown}"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let err = child.stderr.take().map(|e| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(e).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
    });
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(line);
            while let Ok(l) = rx.try_recv() {
                log(l);
            }
        }
    }
    if let Some(h) = err {
        let _ = h.join();
    }
    drop(tx);
    for l in rx {
        log(l);
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("{shown} exited with {status}");
    }
    Ok(())
}

/// Creates (or refreshes) the venv at `dir` and installs the deps; returns
/// its interpreter once `import numpy, tokenizers` succeeds there.
pub fn build(tool: &Tool, dir: &Path, log: &mut dyn FnMut(String)) -> Result<PathBuf> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let py = paths::venv_python(dir);
    match tool {
        Tool::Uv(uv) => {
            let mut venv = Command::new(uv);
            venv.args(["venv", "--allow-existing", "--quiet"]).arg(dir);
            stream(venv, log)?;
            let mut pip = Command::new(uv);
            pip.args(["pip", "install", "--python"]).arg(&py).args(DEPS);
            stream(pip, log)?;
        }
        Tool::Pip(base) => {
            if !py.is_file() {
                let mut venv = Command::new(base);
                venv.args(["-m", "venv"]).arg(dir);
                stream(venv, log)?;
            }
            let mut pip = Command::new(&py);
            pip.args([
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--progress-bar",
                "off",
            ])
            .args(DEPS);
            stream(pip, log)?;
        }
    }
    let mut check = Command::new(&py);
    check.args(["-c", "import numpy, tokenizers; print('numpy', numpy.__version__, '· tokenizers', tokenizers.__version__)"]);
    stream(check, log).context("the new env cannot import numpy + tokenizers")?;
    Ok(py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_wins_over_pip_and_nothing_means_none() {
        let t = Tool::pick(Some(Path::new("/x/uv")), Some("python3")).unwrap();
        assert_eq!(t.name(), "uv");
        assert_eq!(Tool::pick(None, Some("python3")).unwrap().name(), "pip");
        assert!(Tool::pick(None, None).is_none());
    }

    #[test]
    fn stream_forwards_both_pipes_and_reports_failure() {
        if !cfg!(unix) {
            return;
        }
        let mut lines = Vec::new();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err 1>&2"]);
        stream(cmd, &mut |l| lines.push(l)).unwrap();
        assert!(lines.iter().any(|l| l == "out"));
        assert!(lines.iter().any(|l| l == "err"));
        let mut fail = Command::new("sh");
        fail.args(["-c", "exit 3"]);
        assert!(stream(fail, &mut |_| {}).is_err());
    }
}
