//! `urna setup` end to end: the real binary against a `file://` release
//! built here (a payload tarball and its `.sha256`, the layout the release
//! workflow publishes), so the download, the checksum, the unpack and the
//! typed exit codes run exactly as they do against github. needs `curl` on
//! PATH (every ci runner has it); skips cleanly without it.
#![cfg(feature = "tui")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failing unwrap is a failing test"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

const PAYLOAD: &str = "urna-embedder-payload.tar.gz";

/// `file://` url for a local dir (`file:///C:/...` on windows).
fn file_url(p: &Path) -> String {
    let s = p.display().to_string().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn has_curl() -> bool {
    Command::new("curl").arg("--version").output().is_ok()
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("urna_setup_e2e_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// This binary's version, the payload version `setup` asks for by default.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The files a complete payload holds besides the stamp (mirrors
/// `unpack::REQUIRED`, which this binary-crate test cannot import; a drift
/// fails the install below).
const REQUIRED: [&str; 19] = [
    "model_fingerprint.py",
    "embed_query.py",
    "forge/__init__.py",
    "forge/embed_default.py",
    "forge/embed_image.py",
    "forge/embed_potion.py",
    "forge/embed_query_model.py",
    "forge/embed_query_potion.py",
    "forge/embed_st.py",
    "forge/embed_st_worker.py",
    "forge/model_adapters.py",
    "forge/model_registry.py",
    "forge/models/potion-base-8M/README.md",
    "forge/models/potion-base-8M/config.json",
    "forge/models/potion-base-8M/model.safetensors",
    "forge/models/potion-base-8M/modules.json",
    "forge/models/potion-base-8M/special_tokens_map.json",
    "forge/models/potion-base-8M/tokenizer.json",
    "forge/models/potion-base-8M/tokenizer_config.json",
];

/// A release dir with a complete payload stamped with this binary's
/// version; `sha` overrides the published digest (a tampered release).
fn release(dir: &Path, sha: Option<&str>) -> PathBuf {
    release_at(dir, "release", sha, VERSION, None)
}

/// A release under `dir/<name>` whose payload is stamped `version`, every
/// file's body naming that release; `drop` leaves one required file out.
fn release_at(
    dir: &Path,
    name: &str,
    sha: Option<&str>,
    version: &str,
    drop: Option<&str>,
) -> PathBuf {
    let rel = dir.join(name);
    std::fs::create_dir_all(&rel).unwrap();
    let tgz = rel.join(PAYLOAD);
    let gz = GzEncoder::new(std::fs::File::create(&tgz).unwrap(), Compression::fast());
    let mut b = tar::Builder::new(gz);
    let mut entries: Vec<(String, Vec<u8>)> = REQUIRED
        .iter()
        .filter(|f| Some(**f) != drop)
        .map(|f| (format!("urna/{f}"), format!("{f}@{version}").into_bytes()))
        .collect();
    entries.push(("urna/VERSION".into(), format!("{version}\n").into_bytes()));
    for (name, body) in &entries {
        let (name, body) = (name.as_str(), body.as_slice());
        let mut h = tar::Header::new_gnu();
        h.set_size(body.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, name, body).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap();
    let digest = hex::encode(Sha256::digest(std::fs::read(&tgz).unwrap()));
    let line = format!("{} *{PAYLOAD}\n", sha.unwrap_or(&digest));
    std::fs::write(rel.join(format!("{PAYLOAD}.sha256")), line).unwrap();
    rel
}

/// Runs the binary with the data dir and the release pointed into `dir`,
/// no interpreter pinned and no terminal (plain mode).
fn urna(dir: &Path, rel: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_urna"))
        .args(args)
        .current_dir(dir)
        .env("URNA_DATA_DIR", dir.join("data"))
        .env("URNA_RELEASE_BASE", file_url(rel))
        .env("NO_COLOR", "1")
        .env_remove("URNA_PYTHON")
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn setup_yes_installs_the_payload_from_the_release() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("ok");
    let rel = release(&d, None);
    // a venv from an earlier setup: the payload step must leave it alone.
    std::fs::create_dir_all(d.join("data/urna/venv/bin")).unwrap();
    std::fs::write(d.join("data/urna/venv/bin/marker"), b"keep").unwrap();
    let out = urna(&d, &rel, &["setup", "--yes", "--force", "--no-python"]);
    let s = text(&out);
    assert!(s.contains("ok embedder payload"), "{s}");
    assert!(d.join("data/urna/forge/embed_query_potion.py").is_file());
    assert!(d.join("data/urna/model_fingerprint.py").is_file(), "{s}");
    assert!(d.join("data/urna/embed_query.py").is_file(), "{s}");
    assert!(d.join("data/urna/venv/bin/marker").is_file(), "{s}");
    // the stub payload has no potion table, so verify fails with a doctor
    // code (2..=6), never a setup code: the steps themselves succeeded.
    let code = out.status.code().unwrap();
    assert!((0..=6).contains(&code), "exit {code}: {s}");
    assert!(!s.contains('\x1b'), "plain mode printed an escape");
}

#[test]
fn an_upgraded_binary_replaces_an_older_payload_and_keeps_the_venv() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("upgrade");
    // the state a 0.5.1 install leaves: forge/ only (0.5.1's setup dropped the
    // top-level files and its payload had no stamp), and a venv.
    let forge = d.join("data/urna/forge");
    std::fs::create_dir_all(&forge).unwrap();
    std::fs::write(forge.join("embed_query_potion.py"), b"0.5.1").unwrap();
    std::fs::create_dir_all(d.join("data/urna/venv/bin")).unwrap();
    std::fs::write(d.join("data/urna/venv/bin/marker"), b"keep").unwrap();

    // no --force: the plan sees the unstamped payload and replaces it.
    let new = release(&d, None);
    let out = urna(&d, &new, &["setup", "--yes", "--no-python"]);
    let s = text(&out);
    assert!(s.contains("ok embedder payload"), "{s}");
    let stamp = std::fs::read_to_string(d.join("data/urna/VERSION")).unwrap();
    assert_eq!(stamp.trim(), VERSION);
    assert!(d.join("data/urna/embed_query.py").is_file());
    assert!(d.join("data/urna/venv/bin/marker").is_file());

    // the payload matches the binary now: nothing to replace.
    let again = text(&urna(&d, &new, &["setup", "--yes", "--no-python"]));
    assert!(!again.contains("ok embedder payload"), "{again}");

    // --version names another release: replaced again, venv still there.
    let pinned = text(&urna(
        &d,
        &new,
        &["setup", "--yes", "--no-python", "--version", "v9.9.9"],
    ));
    assert!(pinned.contains("ok embedder payload"), "{pinned}");
    assert!(d.join("data/urna/venv/bin/marker").is_file());
}

#[test]
fn setup_repairs_a_payload_missing_a_required_file() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("repair");
    let rel = release(&d, None);
    urna(&d, &rel, &["setup", "--yes", "--no-python"]);
    // each of these breaks a query when gone: search-text's embedder, the
    // module the potion route imports, the potion table's tokenizer.
    for gone in [
        "embed_query.py",
        "forge/embed_potion.py",
        "forge/models/potion-base-8M/tokenizer.json",
    ] {
        std::fs::remove_file(d.join("data/urna").join(gone)).unwrap();
        // same release, no --force: the missing file alone makes setup reinstall.
        let s = text(&urna(&d, &rel, &["setup", "--yes", "--no-python"]));
        assert!(s.contains(&format!("missing {gone}")), "{gone}: {s}");
        assert!(s.contains("ok embedder payload"), "{gone}: {s}");
        assert!(
            d.join("data/urna").join(gone).is_file(),
            "{gone} not repaired"
        );
    }
}

#[test]
fn an_incomplete_release_fails_and_keeps_the_installed_payload() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("halfrel");
    let old = release_at(&d, "old", None, "0.0.1", None);
    urna(
        &d,
        &old,
        &["setup", "--yes", "--no-python", "--version", "0.0.1"],
    );
    let half = release_at(
        &d,
        "half",
        None,
        VERSION,
        Some("forge/embed_query_model.py"),
    );
    let out = urna(&d, &half, &["setup", "--yes", "--no-python"]);
    assert_eq!(out.status.code(), Some(12), "{}", text(&out));
    assert!(
        text(&out).contains("forge/embed_query_model.py"),
        "{}",
        text(&out)
    );
    let stamp = std::fs::read_to_string(d.join("data/urna/VERSION")).unwrap();
    assert_eq!(stamp.trim(), "0.0.1");
    let model = std::fs::read(d.join("data/urna/forge/embed_query_model.py")).unwrap();
    assert_eq!(model, b"forge/embed_query_model.py@0.0.1");
}

#[test]
fn a_tampered_checksum_exits_11_and_installs_nothing() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("sha");
    let rel = release(&d, Some(&"0".repeat(64)));
    let out = urna(&d, &rel, &["setup", "--yes", "--force", "--no-python"]);
    assert_eq!(out.status.code(), Some(11), "{}", text(&out));
    assert!(!d.join("data/urna/forge").exists());
    assert!(text(&out).contains("does not match the release checksum"));
}

#[test]
fn a_missing_release_exits_10() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("gone");
    let out = urna(
        &d,
        &d.join("no-such-release"),
        &["setup", "--yes", "--force", "--no-python"],
    );
    assert_eq!(out.status.code(), Some(10), "{}", text(&out));
}

#[test]
fn uninstall_removes_the_payload_and_keeps_the_binary() {
    if !has_curl() {
        eprintln!("skip: no curl on PATH");
        return;
    }
    let d = scratch("rm");
    let rel = release(&d, None);
    urna(&d, &rel, &["setup", "--yes", "--force", "--no-python"]);
    assert!(d.join("data/urna/forge").is_dir());
    assert!(d.join("data/urna/embed_query.py").is_file());
    let out = urna(&d, &rel, &["setup", "--uninstall"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!d.join("data/urna/forge").exists());
    for f in ["model_fingerprint.py", "embed_query.py", "VERSION"] {
        assert!(!d.join("data/urna").join(f).exists(), "{f} left behind");
    }
    assert!(Path::new(env!("CARGO_BIN_EXE_urna")).is_file());
}

#[test]
fn a_bare_urna_without_a_terminal_prints_help_and_exits_2() {
    let d = scratch("bare");
    let out = urna(&d, &d, &[]);
    assert_eq!(out.status.code(), Some(2));
    let all = format!("{}{}", text(&out), String::from_utf8_lossy(&out.stderr));
    assert!(all.contains("Usage: urna"), "{all}");
    assert!(all.contains("setup"));
}

#[test]
fn doctor_through_a_pipe_has_no_escapes_and_names_setup() {
    let d = scratch("doc");
    let out = Command::new(env!("CARGO_BIN_EXE_urna"))
        .arg("doctor")
        .current_dir(&d)
        .env("URNA_DATA_DIR", d.join("data"))
        .env("URNA_PYTHON", "/urna/no/such/python")
        .output()
        .unwrap();
    let s = text(&out);
    assert_eq!(out.status.code(), Some(2), "{s}");
    assert!(!s.contains('\x1b'));
    assert!(s.contains("urna setup"), "{s}");
}
