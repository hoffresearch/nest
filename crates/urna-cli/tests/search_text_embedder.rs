//! `search-text` finds its sentence-transformers embedder outside a checkout:
//! the payload lays `embed_query.py` down at `<data root>/urna/`, beside
//! `forge/`, and the resolution ladder reaches it there. the binary is copied
//! out of `target/` first, because a dev-built binary also resolves against
//! its own checkout and would always find the repo's copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failing unwrap is a failing test"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("urna_st_embedder_{tag}_{}", std::process::id()));
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

/// `search-text` on the golden fixture from `cwd`, every data root pointed
/// into `dir`; the interpreter is a path that does not exist, so the run
/// stops right after the embedder is resolved. returns stderr.
fn search_text(bin: &Path, dir: &Path, cwd: &Path) -> String {
    let fixture = repo().join("crates/urna-format/tests/fixtures/golden_v1_minimal.urna");
    let out = Command::new(bin)
        .args(["search-text", fixture.to_str().unwrap(), "q"])
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

fn lay_down(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let dst = root.join("embed_query.py");
    std::fs::copy(repo().join("python/embed_query.py"), &dst).unwrap();
    dst
}

#[test]
fn search_text_finds_the_embedder_in_the_data_root() {
    let dir = scratch("data_root");
    let bin = detached_binary(&dir);
    let script = lay_down(&dir.join("data").join("urna"));
    let err = search_text(&bin, &dir, &dir);
    assert!(
        err.contains(&format!("via {}", script.display())),
        "expected the data-root script, got: {err}"
    );
    assert!(!err.contains("embedder script not found"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_text_without_a_payload_names_the_missing_script() {
    let dir = scratch("missing");
    let bin = detached_binary(&dir);
    let err = search_text(&bin, &dir, &dir);
    assert!(err.contains("embedder script not found"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_checkout_in_the_cwd_wins_over_the_data_root() {
    let dir = scratch("checkout");
    let bin = detached_binary(&dir);
    lay_down(&dir.join("data").join("urna"));
    let checkout = dir.join("checkout");
    lay_down(&checkout.join("python"));
    let err = search_text(&bin, &dir, &checkout);
    // the cwd comes back canonical (/private/var on macos): match the tail.
    let tail = Path::new("checkout").join("python").join("embed_query.py");
    assert!(
        err.contains(&tail.display().to_string()),
        "expected the checkout's script, got: {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
