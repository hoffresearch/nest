//! `ask`, `retrieve` and `search-text` find their python embedders outside a
//! checkout: the payload lays `embed_query.py` down at `<data root>/urna/`
//! and the registry embedder at `<data root>/urna/forge/`, and the one
//! resolution ladder (`embed_gate::installed_script_in`) reaches both there.
//! the binary is copied out of `target/` first, because a dev-built binary
//! also resolves against its own checkout and would always find the repo's
//! copy; the cwd is a temp dir, never the repo.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failing unwrap is a failing test"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("urna_resolve_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The binary copied to `<dir>/bin/`, so no checkout sits above it.
fn detached_binary(dir: &Path) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_urna"));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let dst = bin.join(src.file_name().unwrap());
    std::fs::copy(&src, &dst).unwrap();
    dst
}

/// `<verb> <golden fixture> q` from `cwd`, every data root pointed into
/// `dir`; the interpreter is a path that does not exist, so the run stops
/// right after the embedder is resolved and names it. returns stderr.
fn run(bin: &Path, dir: &Path, cwd: &Path, verb: &str) -> String {
    let fixture = repo().join("crates/urna-format/tests/fixtures/golden_v1_minimal.urna");
    let out = Command::new(bin)
        .args([verb, fixture.to_str().unwrap(), "q"])
        .current_dir(cwd)
        .env("URNA_DATA_DIR", dir.join("data"))
        .env("XDG_DATA_HOME", dir.join("xdg"))
        .env("HOME", dir.join("home"))
        .env_remove("LOCALAPPDATA")
        .env("URNA_PYTHON", dir.join("no-such-python"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The payload's two query entry points under `urna_home`, the layout
/// `urna setup` lays down: `embed_query.py` at the top, the registry
/// embedder in `forge/`.
fn lay_down(urna_home: &Path) {
    std::fs::create_dir_all(urna_home.join("forge")).unwrap();
    for rel in ["embed_query.py", "forge/embed_query_model.py"] {
        std::fs::copy(repo().join("python").join(rel), urna_home.join(rel)).unwrap();
    }
}

fn tail(parts: &[&str]) -> String {
    parts.iter().collect::<PathBuf>().display().to_string()
}

#[test]
fn every_query_verb_finds_its_embedder_in_the_data_root() {
    let dir = scratch("data_root");
    let bin = detached_binary(&dir);
    lay_down(&dir.join("data").join("urna"));
    let home = dir.join("data").join("urna");
    let cases = [
        ("search-text", home.join("embed_query.py")),
        ("ask", home.join("forge").join("embed_query_model.py")),
        ("retrieve", home.join("forge").join("embed_query_model.py")),
    ];
    for (verb, script) in cases {
        let err = run(&bin, &dir, &dir, verb);
        assert!(
            err.contains(&script.display().to_string()),
            "{verb}: expected {}, got: {err}",
            script.display()
        );
        assert!(!err.contains("embedder script not found"), "{verb}: {err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_payload_every_verb_names_the_missing_script() {
    let dir = scratch("missing");
    let bin = detached_binary(&dir);
    for (verb, script) in [
        ("search-text", tail(&["python", "embed_query.py"])),
        ("ask", tail(&["python", "forge", "embed_query_model.py"])),
        (
            "retrieve",
            tail(&["python", "forge", "embed_query_model.py"]),
        ),
    ] {
        let err = run(&bin, &dir, &dir, verb);
        assert!(err.contains("embedder script not found"), "{verb}: {err}");
        assert!(err.contains(&script), "{verb}: {err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_checkout_in_the_cwd_wins_over_the_data_root() {
    let dir = scratch("checkout");
    let bin = detached_binary(&dir);
    lay_down(&dir.join("data").join("urna"));
    let checkout = dir.join("checkout");
    lay_down(&checkout.join("python"));
    // the cwd comes back canonical (/private/var on macos): match the tail.
    for (verb, rel) in [
        (
            "search-text",
            tail(&["checkout", "python", "embed_query.py"]),
        ),
        (
            "ask",
            tail(&["checkout", "python", "forge", "embed_query_model.py"]),
        ),
    ] {
        let err = run(&bin, &dir, &checkout, verb);
        assert!(
            err.contains(&rel),
            "{verb}: expected the checkout's script, got: {err}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
