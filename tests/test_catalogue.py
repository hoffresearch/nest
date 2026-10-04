"""Prove the model catalog setup and the explorer offer is the registry's.

`python/forge/model_catalog.py` generates `python/forge/catalog.json` from
the registry and the payload ships it; the installer reads nothing else.

- happy path: the checked-in catalog is exactly what the registry
  generates; the MiniLM the pt-br corpora use is offered with its pinned
  revision, files and model_hash; generation needs no numpy (the release
  stages the payload under a bare python3);
- error path: every preset not offered is listed with a reason (heavy,
  unreviewed remote code, no pinned revision), a stale catalog fails the
  check, and two offered models pinning one package to different versions
  keep only the first;
- edge: the test-only fake preset is in neither list.

Run: .venv/bin/python tests/test_catalogue.py
"""

import dataclasses
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))

from forge import model_catalog as mc
from forge import model_registry as mr

MINILM = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"
MINILM_HASH = "sha256:97a500de1ad2dc9ffb41fbe3dec27d4fd26ddaa13143aa50f905d3d1f6eff0af"


def test_checked_in_catalog_is_the_registrys() -> None:
    have = json.loads(mc.CATALOG.read_text())
    assert have == mc.build(), "python/forge/catalog.json is stale"
    assert mc.main(["--check"]) == 0
    # the release's stage step runs under a python3 without numpy.
    script = str(mc.CATALOG.with_name("model_catalog.py"))
    blocked = subprocess.run(
        [
            sys.executable,
            "-c",
            "import sys, runpy; sys.modules['numpy'] = None; sys.argv = ['c', '--check'];"
            f"runpy.run_path({script!r}, run_name='__main__')",
        ],
        capture_output=True,
        text=True,
    )
    assert blocked.returncode == 0, blocked.stderr
    print("happy path (catalog.json == registry output, generated without numpy): OK")


def test_minilm_is_offered_pinned() -> None:
    cat = mc.build()
    by = {m["name"]: m for m in cat["models"]}
    m = by["minilm-multilingual"]
    assert m["embedding_model"] == m["repo"] == MINILM
    assert len(m["revision"]) == 40 and m["model_hash"] == MINILM_HASH
    paths = [f["path"] for f in m["files"]]
    assert "model.safetensors" in paths and "pytorch_model.bin" not in paths
    assert m["bytes"] == sum(f["size"] for f in m["files"]) > 400_000_000
    assert m["packages"] == ["sentence-transformers>=3"] and not m["remote_code"]
    print("happy path (minilm-multilingual offered at a pinned revision and hash): OK")


def test_every_other_preset_says_why() -> None:
    cat = mc.build()
    offered = {m["name"] for m in cat["models"]}
    excluded = {e["name"]: e["reason"] for e in cat["excluded"]}
    real = {n for n, p in mr.PRESETS.items() if p.kind != "fake"}
    assert offered | set(excluded) == real and not offered & set(excluded)
    assert "fake-test" not in offered | set(excluded)
    assert all(r.strip() for r in excluded.values())
    assert "too heavy" in excluded["wemm-4b"]
    assert (
        "remote code" in excluded["jina-v5-omni-nano"]
        or "model-repo code" in excluded["jina-v5-omni-nano"]
    )
    assert "no pinned hub revision" in excluded["clip-vit-b32"]
    # siglip2 pins a snapshot but the installer cannot verify a tensor hash
    assert "pinned hub snapshot" in excluded["siglip2"], excluded["siglip2"]
    assert "no pinned hub revision" not in excluded["siglip2"]
    assert "payload" in excluded["potion"]
    print("error path (each preset not offered carries its reason; fake hidden): OK")


def test_a_stale_catalog_fails_the_check() -> None:
    with tempfile.TemporaryDirectory(prefix="urna-catalog-") as tmp:
        stale = Path(tmp) / "catalog.json"
        stale.write_text(mc.render({"schema": 1, "models": [], "excluded": []}))
        saved = mc.CATALOG
        mc.CATALOG = stale
        try:
            assert mc.main(["--check"]) == 1
            assert mc.main(["--write"]) == 0 and mc.main(["--check"]) == 0
        finally:
            mc.CATALOG = saved
    print("error path (a stale catalog fails --check, --write fixes it): OK")


def test_conflicting_pins_keep_the_first() -> None:
    base = mr.PRESETS["minilm-multilingual"]
    spec = base.install
    a = dataclasses.replace(
        base,
        name="a",
        embedding_model="a/a",
        install=dataclasses.replace(spec, packages=("transformers==5.2.0",)),
    )
    b = dataclasses.replace(
        base,
        name="b",
        embedding_model="b/b",
        install=dataclasses.replace(spec, packages=("transformers==4.57.0",)),
    )
    c = dataclasses.replace(
        base,
        name="c",
        embedding_model="c/c",
        install=dataclasses.replace(spec, packages=("Transformers==5.2.0",)),
    )
    cat = mc.build({"a": a, "b": b, "c": c})
    assert [m["name"] for m in cat["models"]] == ["a", "c"]
    (why,) = [e["reason"] for e in cat["excluded"] if e["name"] == "b"]
    assert "transformers==4.57.0" in why and "a's" in why, why
    print("edge (two models pinning one package differently: the second is excluded): OK")


def main() -> None:
    test_checked_in_catalog_is_the_registrys()
    test_minilm_is_offered_pinned()
    test_every_other_preset_says_why()
    test_a_stale_catalog_fails_the_check()
    test_conflicting_pins_keep_the_first()
    print("all model catalog tests passed")


if __name__ == "__main__":
    main()
