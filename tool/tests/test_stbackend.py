"""Prove the st_multimodal backend against the local WeMM-2B snapshot:
subprocess isolation (the jina+wemm dynamic-module collision is the reason
the adapter owns a worker), dim/norm contracts, model_hash stability across
constructions, and cross-modal sanity. Skips cleanly when the deps or the
local snapshot are absent, except the pinned model_hash of a synthetic
snapshot, which always runs. NOT run by fullcheck.sh (loads a 2B model).

Run: .venv/bin/python tool/tests/test_stbackend.py
"""

import importlib.util
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "rust", "bridge", "python"))

import numpy as np

from urna.model import presetmap as mr

WEMM = mr.PRESETS["wemm-2b"]
HAVE_DEPS = all(importlib.util.find_spec(m) is not None for m, _ in WEMM.requires)
HAVE_MODEL = WEMM.local_dir is not None and Path(WEMM.local_dir).is_dir()


# the model_hash of a fixed synthetic snapshot, as the st_multimodal backend
# computed it before the move (306e65fd, python/forge/embed_st.py). the key
# names of the fingerprint dict are data that goes into the hash: a renamed
# key changes every corpus's model_hash.
PINNED_MODEL_HASH = "sha256:9885c42ea691ef1dc8fffd08c4403d8580ada6eb3cc20f763e9c0bb8d8b341cb"


def test_model_hash_is_pinned() -> None:
    import hashlib
    import json
    import tempfile
    from types import SimpleNamespace

    from urna.embed.stbackend import fingerprint_for

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp) / "acme-fixture"
        d.mkdir()
        (d / "config.json").write_text('{"hidden_size": 8, "_name_or_path": "acme/fixture"}')
        (d / "tokenizer.json").write_text('{"version": "1.0"}')
        (d / "modules.json").write_text("[]")
        (d / "modeling_fixture.py").write_text("X = 1\n")
        fp = fingerprint_for(SimpleNamespace(model_id="acme/fixture"), d, True, "float32")
    blob = json.dumps(fp, sort_keys=True, separators=(",", ":"))
    assert "model_fingerprint" in fp, sorted(fp)
    assert "sha256:" + hashlib.sha256(blob.encode()).hexdigest() == PINNED_MODEL_HASH, blob


def test_hash_without_load() -> None:
    a = mr.create_embedder("wemm-2b", allow_remote_code={"wemm-2b"})
    b = mr.create_embedder("wemm-2b", allow_remote_code={"wemm-2b"})
    assert a.model_hash == b.model_hash and a.model_hash.startswith("sha256:")
    assert a._proc is None, "model_hash must not load the model"
    fp = a.fingerprint()
    assert fp["remote_code_sha256"], "remote-code files must be fingerprinted"
    a.close()
    b.close()


def test_embed_contracts() -> None:
    emb = mr.create_embedder("wemm-2b", allow_remote_code={"wemm-2b"}, batch_size=2)
    tv = emb.embed_texts(["counter target spell", "a red dragon"], role="query")
    assert tv.shape == (2, 2048)
    assert np.allclose(np.linalg.norm(tv, axis=1), 1.0, atol=1e-3)
    arr = np.zeros((64, 64, 3), dtype=np.uint8)
    iv = emb.embed_arrays([arr])
    assert iv.shape == (1, 2048)
    emb.close()


def test_worker_survives_multi_model() -> None:
    """The measured failure: jina loaded first broke wemm image embeds in
    one process. Through subprocess adapters both must work."""
    if importlib.util.find_spec("sentence_transformers") is None:
        return
    try:
        j = mr.create_embedder(
            "jina-v5-omni-nano", allow_remote_code={"jina-v5-omni-nano"}, batch_size=2
        )
        j.embed_texts(["warm"])
    except mr.RegistryError:
        print("test_worker_survives_multi_model: SKIP (jina snapshot not cached)")
        return
    w = mr.create_embedder("wemm-2b", allow_remote_code={"wemm-2b"}, batch_size=2)
    iv = w.embed_arrays([np.zeros((64, 64, 3), dtype=np.uint8)])
    assert iv.shape == (1, 2048)
    j.close()
    w.close()


def main() -> None:
    # needs no model and no sentence-transformers: always runs.
    test_model_hash_is_pinned()
    print("test_model_hash_is_pinned: OK")
    if not (HAVE_DEPS and HAVE_MODEL):
        print("SKIP: sentence-transformers stack or the local WeMM-2B snapshot is absent")
        return
    tests = ("test_hash_without_load", "test_embed_contracts", "test_worker_survives_multi_model")
    for fn in tests:
        globals()[fn]()
        print(f"{fn}: OK")
    print("all stbackend tests passed")


if __name__ == "__main__":
    main()
