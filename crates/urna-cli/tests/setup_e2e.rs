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

/// A release dir with a payload holding `urna/forge/embed_query_potion.py`;
/// `sha` overrides the published digest (a tampered release).
fn release(dir: &Path, sha: Option<&str>) -> PathBuf {
    let rel = dir.join("release");
    std::fs::create_dir_all(&rel).unwrap();
    let tgz = rel.join(PAYLOAD);
    let gz = GzEncoder::new(std::fs::File::create(&tgz).unwrap(), Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (name, body) in [
        ("urna/forge/embed_query_potion.py", &b"print('probe')\n"[..]),
        ("urna/forge/__init__.py", &b""[..]),
    ] {
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
        return;
    }
    let d = scratch("ok");
    let rel = release(&d, None);
    let out = urna(&d, &rel, &["setup", "--yes", "--force", "--no-python"]);
    let s = text(&out);
    assert!(s.contains("ok embedder payload"), "{s}");
    assert!(d.join("data/urna/forge/embed_query_potion.py").is_file());
    // the stub payload has no potion table, so verify fails with a doctor
    // code (2..=6), never a setup code: the steps themselves succeeded.
    let code = out.status.code().unwrap();
    assert!((0..=6).contains(&code), "exit {code}: {s}");
    assert!(!s.contains('\x1b'), "plain mode printed an escape");
}

#[test]
fn a_tampered_checksum_exits_11_and_installs_nothing() {
    if !has_curl() {
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
        return;
    }
    let d = scratch("rm");
    let rel = release(&d, None);
    urna(&d, &rel, &["setup", "--yes", "--force", "--no-python"]);
    assert!(d.join("data/urna/forge").is_dir());
    let out = urna(&d, &rel, &["setup", "--uninstall"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!d.join("data/urna/forge").exists());
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
