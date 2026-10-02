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

/// The files of a complete payload (mirrors `cmd::payload::REQUIRED`, which
/// this binary-crate test cannot import; the payload test pins that list to
/// what the stage script ships).
const REQUIRED: [&str; 20] = [
    "VERSION",
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

/// A complete payload under `home`, built from the checkout's own files
/// (the table hard-linked, not copied), minus `drop`.
fn lay_down_payload(home: &Path, drop: &str) {
    for rel in REQUIRED.iter().filter(|r| **r != drop) {
        let dst = home.join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        if *rel == "VERSION" {
            std::fs::write(&dst, format!("{}\n", env!("CARGO_PKG_VERSION"))).unwrap();
            continue;
        }
        let src = repo().join("python").join(rel);
        if std::fs::hard_link(&src, &dst).is_err() {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

#[test]
fn a_payload_missing_a_module_names_the_files_and_setup_not_pip() {
    let Some(python) = ["python3", "python"].into_iter().find(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }) else {
        eprintln!("skip: no python on PATH to run the embedder");
        return;
    };
    let dir = scratch("broken_payload");
    let bin = detached_binary(&dir);
    let home = dir.join("data").join("urna");
    lay_down_payload(&home, "forge/model_registry.py");
    let fixture = repo().join("crates/urna-format/tests/fixtures/golden_v1_minimal.urna");
    let out = Command::new(&bin)
        .args(["ask", fixture.to_str().unwrap(), "q"])
        .current_dir(&dir)
        .env("URNA_DATA_DIR", dir.join("data"))
        .env("XDG_DATA_HOME", dir.join("xdg"))
        .env("HOME", dir.join("home"))
        .env_remove("LOCALAPPDATA")
        .env("URNA_PYTHON", python)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        err.contains("payload") && err.contains("is incomplete"),
        "{err}"
    );
    assert!(err.contains("forge/model_registry.py"), "{err}");
    assert!(err.contains(&home.display().to_string()), "{err}");
    assert!(err.contains("urna setup"), "{err}");
    assert!(!err.contains("pip install"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
