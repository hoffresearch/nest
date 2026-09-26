//! What this machine already has, probed once before the plan is shown:
//! how urna got here (the channel), where its data lives, whether the
//! embedder payload and a python with numpy + tokenizers are in place, and
//! which tools the install steps can use (curl, uv, a base python).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cmd::{embed_gate, paths, pyenv};
use crate::tui::hud::Badge;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Homebrew,
    Npm,
    Cargo,
    OneLiner,
    Dev,
    Other,
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Channel::Homebrew => "homebrew",
            Channel::Npm => "npm",
            Channel::Cargo => "cargo",
            Channel::OneLiner => "install script",
            Channel::Dev => "dev build",
            Channel::Other => "manual",
        }
    }

    /// How the binary was installed, read off its own path.
    pub fn of(exe: &Path) -> Channel {
        let s = exe.to_string_lossy().replace('\\', "/");
        if s.contains("/Cellar/") || s.contains("/homebrew/") || s.contains("/linuxbrew/") {
            Channel::Homebrew
        } else if s.contains("/node_modules/") {
            Channel::Npm
        } else if s.contains("/.cargo/bin/") {
            Channel::Cargo
        } else if s.contains("/target/debug/") || s.contains("/target/release/") {
            Channel::Dev
        } else if s.contains("/.local/bin/") {
            Channel::OneLiner
        } else {
            Channel::Other
        }
    }
}

#[derive(Clone, Debug)]
pub struct Scan {
    pub version: &'static str,
    pub target: String,
    pub exe: PathBuf,
    pub channel: Channel,
    /// `<data root>/urna`, where setup writes.
    pub home: Option<PathBuf>,
    /// the embedder script, when one resolves.
    pub embedder: Option<PathBuf>,
    /// interpreter + `--version` of the one the embedder would run under.
    pub python: Option<(String, String)>,
    pub deps: bool,
    /// a python able to create a venv (`python3 -m venv`), when present.
    pub base_python: Option<String>,
    pub uv: Option<PathBuf>,
    pub curl: Option<PathBuf>,
    pub simd: &'static str,
}

/// First match for `name` on PATH (with the PATHEXT suffixes on windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .chain(std::iter::once(String::new()))
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|e| dir.join(format!("{name}{e}")))
            .find(|p| p.is_file())
    })
}

fn output(cmd: &mut Command) -> Option<String> {
    let o = cmd.output().ok().filter(|o| o.status.success())?;
    let mut s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s.is_empty() {
        s = String::from_utf8_lossy(&o.stderr).trim().to_string();
    }
    Some(s)
}

fn base_python() -> Option<String> {
    let names: &[&str] = if cfg!(windows) {
        &["python", "py"]
    } else {
        &["python3", "python"]
    };
    names.iter().find_map(|n| {
        let p = which(n)?;
        let s = p.to_string_lossy().into_owned();
        output(Command::new(&s).args(["-c", "import venv, ensurepip"])).map(|_| s)
    })
}

impl Scan {
    /// Runs every probe. Blocking (it spawns python a few times, well
    /// under a second on a warm machine); the ui calls it off-thread.
    pub fn probe() -> Scan {
        pyenv::set_quiet(true);
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_default();
        let embedder = Some(embed_gate::default_potion_embedder_path()).filter(|p| p.is_file());
        let interp = pyenv::resolve_interpreter();
        let python = output(Command::new(&interp).arg("--version")).map(|v| (interp.clone(), v));
        let deps = python.is_some()
            && output(Command::new(&interp).args(["-c", "import numpy, tokenizers"])).is_some();
        Scan {
            version: env!("CARGO_PKG_VERSION"),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            channel: Channel::of(&exe),
            exe,
            home: paths::urna_home(),
            embedder,
            python,
            deps,
            base_python: base_python(),
            uv: which("uv"),
            curl: which("curl"),
            simd: urna_runtime::simd::detect_backend().name(),
        }
    }

    /// The rows the scan screen reveals, in order: label, value, badge.
    pub fn facts(&self) -> Vec<(&'static str, String, Badge)> {
        let tilde = |p: &Path| tilde(p);
        let mut v = vec![
            (
                "urna",
                format!("{} · {}", self.version, self.target),
                Badge::Ok,
            ),
            (
                "channel",
                format!("{} · {}", self.channel.name(), tilde(&self.exe)),
                Badge::Ok,
            ),
            ("simd", self.simd.to_string(), Badge::Ok),
        ];
        v.push(match &self.home {
            Some(h) => ("data dir", tilde(h), Badge::Ok),
            None => ("data dir", "no HOME, set URNA_DATA_DIR".into(), Badge::Fail),
        });
        v.push(match &self.embedder {
            Some(p) => ("embedder", tilde(p), Badge::Ok),
            None => (
                "embedder",
                "missing, setup downloads it".into(),
                Badge::Warn,
            ),
        });
        v.push(match &self.python {
            Some((i, ver)) => (
                "python",
                format!("{ver} · {}", tilde(Path::new(i))),
                Badge::Ok,
            ),
            None => ("python", "not found".into(), Badge::Warn),
        });
        v.push(if self.deps {
            ("numpy, tokenizers", "present".into(), Badge::Ok)
        } else {
            (
                "numpy, tokenizers",
                "missing, setup builds a venv".into(),
                Badge::Warn,
            )
        });
        let tool = match (&self.uv, &self.base_python) {
            (Some(uv), _) => (format!("uv · {}", tilde(uv)), Badge::Ok),
            (None, Some(py)) => (format!("pip · {}", tilde(Path::new(py))), Badge::Ok),
            (None, None) => ("no uv, no python3".into(), Badge::Fail),
        };
        v.push(("env tool", tool.0, tool.1));
        v.push(match &self.curl {
            Some(c) => ("curl", tilde(c), Badge::Ok),
            None => ("curl", "missing, downloads disabled".into(), Badge::Fail),
        });
        v
    }
}

/// `$HOME/x` as `~/x`, for display only.
pub fn tilde(p: &Path) -> String {
    crate::tui::txt::home(&p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_reads_the_install_path() {
        let c = |s: &str| Channel::of(Path::new(s));
        assert_eq!(
            c("/opt/homebrew/Cellar/urna/0.5.0/bin/urna"),
            Channel::Homebrew
        );
        assert_eq!(
            c("/usr/lib/node_modules/@urna/cli/node_modules/.bin_real/urna"),
            Channel::Npm
        );
        assert_eq!(c("/home/u/.cargo/bin/urna"), Channel::Cargo);
        assert_eq!(c("/home/u/.local/bin/urna"), Channel::OneLiner);
        assert_eq!(c("/src/urna/target/release/urna"), Channel::Dev);
        assert_eq!(c("/usr/bin/urna"), Channel::Other);
    }

    #[test]
    fn which_finds_a_shell_and_misses_nonsense() {
        if cfg!(unix) {
            assert!(which("sh").is_some());
        }
        assert!(which("urna-definitely-not-a-binary-xyz").is_none());
    }

    #[test]
    fn probe_reports_the_running_binary() {
        let s = Scan::probe();
        assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(s.facts()[0].0, "urna");
        assert!(s.facts().len() >= 8);
    }
}
