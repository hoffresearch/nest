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

/// The release the payload comes from: `--version` when given, else this
/// binary's own (a leading `v` is accepted either way).
pub fn wanted_version(scan: &Scan, opts: &Opts) -> String {
    opts.version
        .as_deref()
        .unwrap_or(scan.version)
        .trim_start_matches('v')
        .to_string()
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
        let want = wanted_version(scan, opts);
        // a payload from another release (or from before the stamp) is
        // replaced, so upgrading the binary upgrades the scripts it runs; one
        // missing a required file is repaired the same way.
        let broken = scan.payload.as_ref().filter(|p| !p.missing.is_empty());
        let stale = scan
            .payload
            .as_ref()
            .filter(|p| p.version.as_deref() != Some(want.as_str()));
        // setup manages the payload in its own data dir: one found in
        // another data root (or a checkout) does not count as installed.
        let missing = scan.payload.is_none();
        let fresh = format!(
            "download ~30 MB from the v{want} release, verify sha256, unpack to {home}/forge"
        );
        let detail = match (&scan.embedder, stale, missing) {
            _ if broken.is_some() => format!(
                "installed payload is missing {}: download v{want}, verify sha256, replace {home}/forge (the venv stays)",
                broken.map(|p| p.missing.join(" ")).unwrap_or_default()
            ),
            (_, Some(p), _) => format!(
                "installed payload is {}, v{want} is wanted: download it, verify sha256, replace {home}/forge (the venv stays)",
                p.label()
            ),
            (Some(p), None, true) => format!("{fresh}; {} resolves until then", tilde(p)),
            (Some(p), None, false) => format!("present at {}; tick to reinstall", tilde(p)),
            (None, None, _) => fresh,
        };
        Item {
            task: Task::Payload,
            on: (missing || broken.is_some() || stale.is_some() || opts.force) && !opts.no_payload,
            locked: false,
            blocked,
            detail,
        }
    };
    let python = {
        let tool = Tool::pick(scan.uv.as_deref(), scan.base_python.as_deref());
        let pinned = scan.pinned_python.clone();
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
    use crate::tui::setup::scan::{Channel, Payload};
    use std::path::PathBuf;

    fn bare() -> Scan {
        Scan {
            version: "0.5.0",
            target: "aarch64-macos".into(),
            exe: PathBuf::from("/opt/homebrew/bin/urna"),
            channel: Channel::Homebrew,
            home: Some(PathBuf::from("/h/.local/share/urna")),
            embedder: None,
            payload: None,
            python: Some(("python3".into(), "Python 3.12.4".into())),
            pinned_python: None,
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
        s.payload = Some(Payload {
            version: Some("0.5.0".into()),
            missing: Vec::new(),
        });
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

    fn installed(version: Option<&str>) -> Scan {
        let mut s = bare();
        s.embedder = Some(PathBuf::from(
            "/h/.local/share/urna/forge/embed_query_potion.py",
        ));
        s.payload = Some(Payload {
            version: version.map(String::from),
            missing: Vec::new(),
        });
        s.deps = true;
        s
    }

    #[test]
    fn an_upgraded_binary_replaces_the_payload_of_another_release() {
        // the binary is 0.5.0 (bare); a 0.4.9 payload and an unstamped one
        // (0.5.1's, from before the stamp) are both replaced, the venv step
        // stays off.
        for have in [Some("0.4.9"), None] {
            let p = plan(&installed(have), &Opts::default());
            assert!(p[0].runs(), "{have:?}");
            assert!(p[0].detail.contains("v0.5.0 is wanted"), "{}", p[0].detail);
            assert!(!p[1].runs());
        }
        let unstamped = plan(&installed(None), &Opts::default());
        assert!(unstamped[0].detail.contains("unstamped"));
    }

    #[test]
    fn a_payload_missing_a_required_file_is_repaired() {
        // stamped with the wanted release, but a file is gone: the payload
        // step runs and names the file; the venv step stays off.
        let mut s = installed(Some("0.5.0"));
        if let Some(p) = s.payload.as_mut() {
            p.missing = vec!["embed_query.py".into()];
        }
        let p = plan(&s, &Opts::default());
        assert!(p[0].runs(), "{}", p[0].detail);
        assert!(
            p[0].detail.contains("missing embed_query.py"),
            "{}",
            p[0].detail
        );
        assert!(!p[1].runs());
        let skip = Opts {
            no_payload: true,
            ..Opts::default()
        };
        assert!(!plan(&s, &skip)[0].runs());
    }

    #[test]
    fn a_payload_in_another_data_root_does_not_count_as_installed() {
        // URNA_DATA_DIR points setup at an empty dir while an older install
        // sits in ~/.local/share: setup installs into its own dir.
        let mut s = bare();
        s.embedder = Some(PathBuf::from(
            "/h/.local/share/urna/forge/embed_query_potion.py",
        ));
        s.home = Some(PathBuf::from("/tmp/data/urna"));
        s.deps = true;
        let p = plan(&s, &Opts::default());
        assert!(p[0].runs(), "{}", p[0].detail);
        assert!(
            p[0].detail.contains("resolves until then"),
            "{}",
            p[0].detail
        );
    }

    #[test]
    fn version_picks_the_payload_release_and_matching_skips_it() {
        let pinned = Opts {
            version: Some("v0.4.9".into()),
            ..Opts::default()
        };
        assert_eq!(wanted_version(&bare(), &pinned), "0.4.9");
        assert!(!plan(&installed(Some("0.4.9")), &pinned)[0].runs());
        assert!(plan(&installed(Some("0.5.0")), &pinned)[0].runs());
        let fresh = plan(&bare(), &pinned);
        assert!(
            fresh[0].detail.contains("v0.4.9 release"),
            "{}",
            fresh[0].detail
        );
        let skip = Opts {
            no_payload: true,
            ..Opts::default()
        };
        assert!(!plan(&installed(Some("0.4.9")), &skip)[0].runs());
    }

    #[test]
    fn a_pinned_interpreter_without_deps_blocks_the_python_step() {
        // URNA_PYTHON wins over any venv setup would build, so a pinned
        // interpreter lacking the deps is a block with the variable named,
        // and one that has them needs no python step at all. the scan
        // carries the pin; the plan never reads the process env, so this
        // holds whatever the gate exports.
        let mut s = bare();
        s.pinned_python = Some("/opt/py/bin/python".into());
        let p = plan(&s, &Opts::default());
        assert!(p[1].blocked.as_deref().unwrap().contains("URNA_PYTHON"));
        assert!(p[1].detail.contains("/opt/py/bin/python"));
        assert_eq!(blocked_code(&p), 14);
        s.deps = true;
        let p = plan(&s, &Opts::default());
        assert!(p[1].blocked.is_none());
        assert!(!p[1].runs());
        assert_eq!(blocked_code(&p), 0);
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
