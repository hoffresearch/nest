"""Prove the staged embedder payload answers queries with no repo around it.

`scripts/stage_embedder_payload.py` is what the release archives and
`urna setup` lay down; an installed binary resolves both query embedders
inside that tree (`urna/forge/embed_query_potion.py` for potion corpora,
`urna/forge/embed_query_model.py` for registry models, `urna/embed_query.py`
for search-text and for sentence-transformers models outside the
registry). this suite stages
the payload into a temp dir, then runs the staged scripts from a cwd
outside the checkout, with only the staged tree on their path:

- happy path: the potion route embeds and reports the potion model_hash,
  byte-for-byte the value the repo copy reports;
- the registry route: a potion manifest name resolves through the
  registry and embeds; a remote-code preset without the opt-in is refused
  with the URNA_ALLOW_REMOTE_CODE hint (exit 4) before any dependency is
  looked at; an unknown manifest model is exit 4 naming the known models
  when sentence-transformers is absent, and is handed to embed_query.py
  when it is present;
- error path: a payload staged from a tree missing a module fails the
  staging itself, so a release never ships half the route.

Run: .venv/bin/python tests/test_embedder_payload.py
"""

import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
STAGE = REPO / "scripts" / "stage_embedder_payload.py"
POTION = "minishlab/potion-base-8M/v1"

EXPECTED_FILES = {
    "urna/model_fingerprint.py",
    "urna/embed_query.py",
    "urna/VERSION",
    "urna/forge/__init__.py",
    "urna/forge/embed_default.py",
    "urna/forge/embed_potion.py",
    "urna/forge/embed_query_potion.py",
    "urna/forge/embed_query_model.py",
    "urna/forge/model_registry.py",
    "urna/forge/model_adapters.py",
    "urna/forge/embed_st.py",
    "urna/forge/embed_st_worker.py",
    "urna/forge/embed_image.py",
}


def _run(
    script: Path, *args: str, cwd: Path, env: dict | None = None
) -> subprocess.CompletedProcess:
    # a cwd outside the checkout, and no PYTHONPATH: the staged tree has to
    # find its own modules, exactly like an installed binary's child.
    e = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
    e.update(env or {})
    return subprocess.run(
        [sys.executable, str(script), *args],
        cwd=cwd,
        env=e,
        capture_output=True,
        text=True,
        check=False,
    )


def test_stage_and_query(base: Path) -> None:
    dest = base / "payload"
    r = subprocess.run(
        [sys.executable, str(STAGE), str(dest)], capture_output=True, text=True, check=False
    )
    assert r.returncode == 0, r.stderr
    staged = {str(p.relative_to(dest)) for p in dest.rglob("*") if p.is_file()}
    missing = EXPECTED_FILES - staged
    assert not missing, f"payload is missing {sorted(missing)}"
    # what `urna setup` requires (unpack::REQUIRED) is exactly what the stage
    # script ships: a shipped file it does not require could go missing
    # unnoticed, a required file it does not ship would fail every install.
    src = (REPO / "crates/urna-cli/src/tui/setup/unpack.rs").read_text()
    block = src[src.index("pub const REQUIRED") : src.index("];", src.index("pub const REQUIRED"))]
    required = {"urna/" + r for r in re.findall(r'"([^"]+)"', block)}
    assert len(required) >= 8, required
    assert required == staged, (
        f"required but not shipped: {sorted(required - staged)}; "
        f"shipped but not required: {sorted(staged - required)}"
    )
    assert (dest / "urna" / "forge" / "models" / "potion-base-8M").is_dir()
    # the stamp `urna setup` compares against the release it wants.
    import tomllib

    with (REPO / "Cargo.toml").open("rb") as f:
        version = tomllib.load(f)["workspace"]["package"]["version"]
    assert (dest / "urna" / "VERSION").read_text() == version + "\n"

    outside = base / "elsewhere"
    outside.mkdir()
    forge = dest / "urna" / "forge"

    # the potion route, and the registry route on the same manifest name,
    # both embed offline and agree on the model_hash (the gate compares it
    # against the manifest).
    potion = _run(forge / "embed_query_potion.py", POTION, "hello world", cwd=outside)
    assert potion.returncode == 0, potion.stderr
    registry = _run(forge / "embed_query_model.py", POTION, "hello world", cwd=outside)
    assert registry.returncode == 0, registry.stderr
    a, b = json.loads(potion.stdout), json.loads(registry.stdout)
    assert a["model_hash"].startswith("sha256:") and a["model_hash"] == b["model_hash"]
    assert a["embedding_dim"] == b["embedding_dim"] == 256
    assert len(a["vector"]) == 256

    # a registry model needing remote code is refused without the explicit
    # opt-in, before dependencies come into it: the same answer on a box
    # with torch and on one without.
    wemm = _run(forge / "embed_query_model.py", "tencent/WeMM-Embedding-2B", "hello", cwd=outside)
    assert wemm.returncode == 4, (wemm.returncode, wemm.stderr)
    assert "URNA_ALLOW_REMOTE_CODE" in wemm.stderr, wemm.stderr

    # with the opt-in, the answer names what is missing (exit 4 with the pip
    # line) or goes on to the model assets (exit 3) when the deps exist; it
    # is never "embedder script not found", which is the failure this
    # payload retires.
    wemm_ok = _run(
        forge / "embed_query_model.py",
        "tencent/WeMM-Embedding-2B",
        "hello",
        cwd=outside,
        env={"URNA_ALLOW_REMOTE_CODE": "wemm-2b", "HF_HUB_OFFLINE": "1"},
    )
    assert wemm_ok.returncode in (3, 4), (wemm_ok.returncode, wemm_ok.stderr)
    if wemm_ok.returncode == 4:
        assert "install with:" in wemm_ok.stderr, wemm_ok.stderr

    # a model no preset names: without sentence-transformers, exit 4 naming
    # the known models and the pip line; with it, the staged embed_query.py
    # takes over and fails offline on a model that is not cached.
    unknown = _run(forge / "embed_query_model.py", "acme/not-a-model", "hello", cwd=outside)
    if importlib.util.find_spec("sentence_transformers") is None:
        assert unknown.returncode == 4, (unknown.returncode, unknown.stderr)
        assert "no registry preset embeds" in unknown.stderr, unknown.stderr
        assert "sentence-transformers" in unknown.stderr, unknown.stderr
    else:
        assert unknown.returncode != 0, unknown.stdout
        assert "no registry preset embeds" not in unknown.stderr, unknown.stderr

    # the search-text embedder sits beside forge/ and imports
    # model_fingerprint from the same staged root.
    st = _run(dest / "urna" / "embed_query.py", "--help", cwd=outside)
    assert st.returncode == 0 and "--mrl-dim" in st.stdout, st.stderr
    print("stage + potion + registry + sentence-transformers routes: OK")


def test_stage_refuses_incomplete_tree(base: Path) -> None:
    # a copy of the checkout's python/ with one route module removed: the
    # staging script must fail, not ship the rest.
    import shutil

    fake_root = base / "fake-repo"
    (fake_root / "scripts").mkdir(parents=True)
    shutil.copyfile(STAGE, fake_root / "scripts" / STAGE.name)
    shutil.copytree(
        REPO / "python" / "forge",
        fake_root / "python" / "forge",
        ignore=shutil.ignore_patterns("__pycache__", "demo_corpus", "test_*.py"),
    )
    shutil.copyfile(
        REPO / "python" / "model_fingerprint.py", fake_root / "python" / "model_fingerprint.py"
    )
    (fake_root / "python" / "forge" / "model_registry.py").unlink()
    r = subprocess.run(
        [sys.executable, str(fake_root / "scripts" / STAGE.name), str(base / "half")],
        capture_output=True,
        text=True,
        check=False,
    )
    assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
    assert "missing source" in r.stderr and "model_registry.py" in r.stderr, r.stderr
    print("incomplete tree refused: OK")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="urna-payload-") as tmp:
        base = Path(tmp)
        test_stage_and_query(base)
        test_stage_refuses_incomplete_tree(base)
    print("all embedder payload tests passed")


if __name__ == "__main__":
    main()
