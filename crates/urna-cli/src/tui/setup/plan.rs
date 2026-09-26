//! The plan: which steps run, derived from the scan and the flags. A step
//! that is already satisfied starts unticked (ticking it reinstalls); a
//! step the machine cannot run is blocked with the reason, never hidden.

use super::scan::{Scan, tilde};
use super::venv::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Task {
    Payload,
    Python,
    Verify,
}

impl Task {
    pub fn title(self) -> &'static str {
        match self {
            Task::Payload => "embedder payload",
            Task::Python => "python env",
            Task::Verify => "verify",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Opts {
    /// no questions: run the default plan and print plain progress.
    pub yes: bool,
    /// release tag to fetch the payload from (default: this binary's).
    pub version: Option<String>,
    /// reinstall the payload even when one is present.
    pub force: bool,
    pub no_payload: bool,
    pub no_python: bool,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub task: Task,
    pub on: bool,
    /// runs no matter what (verify).
    pub locked: bool,
    /// why the step cannot run here.
    pub blocked: Option<String>,
    pub detail: String,
}

impl Item {
    pub fn runs(&self) -> bool {
        self.on && self.blocked.is_none()
    }
}

/// 14 when a step the machine needs is blocked here (it wanted to run and
/// cannot), else 0. the run starts from this code; a later failure only
/// replaces a 0.
pub fn blocked_code(items: &[Item]) -> i32 {
    if items.iter().any(|i| i.on && i.blocked.is_some()) {
        super::job::codes::BLOCKED
    } else {
        0
    }
}

pub fn plan(scan: &Scan, opts: &Opts) -> Vec<Item> {
    let home = scan
        .home
        .as_deref()
        .map(tilde)
        .unwrap_or_else(|| "?".into());
    let payload = {
        let blocked = if scan.home.is_none() {
            Some("no data dir: set URNA_DATA_DIR".to_string())
        } else if scan.curl.is_none() {
            Some("curl is not on PATH".to_string())
        } else {
            None
        };
        let detail = match &scan.embedder {
            Some(p) => format!("present at {}; tick to reinstall", tilde(p)),
            None => format!(
                "download ~30 MB from the v{} release, verify sha256, unpack to {home}/forge",
                scan.version
            ),
        };
        Item {
            task: Task::Payload,
            on: (scan.embedder.is_none() || opts.force) && !opts.no_payload,
            locked: false,
            blocked,
            detail,
        }
    };
    let python = {
        let tool = Tool::pick(scan.uv.as_deref(), scan.base_python.as_deref());
        let pinned = std::env::var("URNA_PYTHON").ok().filter(|s| !s.is_empty());
        let detail = match (&scan.python, scan.deps, &tool) {
            (_, false, _) if pinned.is_some() => format!(
                "URNA_PYTHON={} lacks numpy + tokenizers and wins over any env setup builds; unset it or install the deps there",
                pinned.clone().unwrap_or_default()
            ),
            (Some((i, _)), true, _) => format!(
                "{} already imports numpy + tokenizers",
                tilde(std::path::Path::new(i))
            ),
            (_, _, Some(t)) => format!(
                "venv at {home}/venv with numpy + tokenizers, via {}",
                t.name()
            ),
            _ => "needs uv or python3 with venv".into(),
        };
        Item {
            task: Task::Python,
            on: !scan.deps && !opts.no_python,
            locked: false,
            blocked: if tool.is_none() {
                Some("no uv and no python3 on PATH".to_string())
            } else if pinned.is_some() && !scan.deps {
                Some("URNA_PYTHON pins an interpreter without the deps".to_string())
            } else {
                None
            },
            detail,
        }
    };
    let verify = Item {
        task: Task::Verify,
        on: true,
        locked: true,
        blocked: None,
        detail: "urna doctor: interpreter, deps, table, one offline embed".into(),
    };
    vec![payload, python, verify]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::setup::scan::Channel;
    use std::path::PathBuf;

    fn bare() -> Scan {
        Scan {
            version: "0.5.0",
            target: "aarch64-macos".into(),
            exe: PathBuf::from("/opt/homebrew/bin/urna"),
            channel: Channel::Homebrew,
            home: Some(PathBuf::from("/h/.local/share/urna")),
            embedder: None,
            python: Some(("python3".into(), "Python 3.12.4".into())),
            deps: false,
            base_python: Some("python3".into()),
            uv: None,
            curl: Some(PathBuf::from("/usr/bin/curl")),
            simd: "neon",
        }
    }

    #[test]
    fn a_fresh_brew_install_plans_everything() {
        let p = plan(&bare(), &Opts::default());
        assert!(p.iter().all(Item::runs));
        assert!(p[1].detail.contains("via pip"));
    }

    #[test]
    fn a_ready_machine_only_verifies_unless_forced() {
        let mut s = bare();
        s.embedder = Some(PathBuf::from(
            "/h/.local/share/urna/forge/embed_query_potion.py",
        ));
        s.deps = true;
        let p = plan(&s, &Opts::default());
        assert_eq!(p.iter().filter(|i| i.runs()).count(), 1);
        let forced = plan(
            &s,
            &Opts {
                force: true,
                ..Opts::default()
            },
        );
        assert!(forced[0].runs());
    }

    #[test]
    fn missing_tools_block_with_a_reason() {
        let mut s = bare();
        s.curl = None;
        s.base_python = None;
        let p = plan(&s, &Opts::default());
        assert!(p[0].blocked.as_deref().unwrap().contains("curl"));
        assert_eq!(blocked_code(&p), 14);
        assert_eq!(blocked_code(&plan(&bare(), &Opts::default())), 0);
        assert!(!p[1].runs());
        assert!(p[2].runs());
    }
}
