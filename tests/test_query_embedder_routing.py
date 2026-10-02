"""Prove the query-embedder ROUTING (RFC-4): a manifest whose
embedding_model is a registry model routes ask/retrieve through
embed_query_model.py (observable: the fake preset's env gate error surfaces
when the env var is missing, and the query succeeds when it is set); a
corpus built with mrl_dim gets --mrl-dim so the truncated-dim gate passes;
and the sentence-transformers path (embed_query.py, behind search-text)
slices a query the same way the registry does. A manifest model no preset
names falls through to embed_query.py, so ask works on a corpus built with
any sentence-transformers model (cases 6 to 9).
The three-layer gate itself is covered by test_search_text_model_hash.py.

Run: .venv/bin/python tests/test_query_embedder_routing.py
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))
CLI = REPO / "target" / "release" / "urna"
if not CLI.exists():
    raise SystemExit("build the CLI first: cargo build --release --workspace")

os.environ["URNA_ENABLE_FAKE_PRESET"] = "1"

import urna
from forge import model_registry as mr


def _build(out: str, mrl_dim: int | None) -> None:
    emb = mr.create_embedder("fake-test")
    texts = [f"fake chunk number {i}" for i in range(4)]
    vecs = emb.embed_texts(texts)
    chunks = [
        {
            "canonical_text": t,
            "source_uri": "test://f",
            "byte_start": i,
            "byte_end": i + 1,
            "embedding": vecs[i].tolist(),
        }
        for i, t in enumerate(texts)
    ]
    urna.build(
        out,
        emb.embedding_model,
        8,
        "t/1",
        emb.model_hash,
        chunks,
        preset="exact",
        reproducible=True,
        mrl_dim=mrl_dim,
    )


def _ask(corpus: str, env_fake: bool) -> tuple[int, str, str]:
    env = dict(os.environ)
    env["URNA_PYTHON"] = str(REPO / ".venv" / "bin" / "python")
    if not env_fake:
        env.pop("URNA_ENABLE_FAKE_PRESET", None)
    p = subprocess.run(
        [str(CLI), "ask", corpus, "fake chunk number 2", "-k", "1"],
        capture_output=True,
        text=True,
        cwd=REPO,
        env=env,
    )
    return p.returncode, p.stdout, p.stderr


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        corpus = str(Path(tmp) / "r.urna")
        _build(corpus, mrl_dim=None)

        rc, out, err = _ask(corpus, env_fake=True)
        assert rc == 0, err
        assert "fake chunk number" in out and "urna://" in out
        print("case 1 (registry model routed + gate passes): OK")

        rc, _, err = _ask(corpus, env_fake=False)
        assert rc != 0 and "URNA_ENABLE_FAKE_PRESET" in err, (
            "the failure must come from embed_query_model.py's registry path, "
            f"proving the routing; got: {err}"
        )
        print("case 2 (routing observable via registry error): OK")

        mrl = str(Path(tmp) / "mrl.urna")
        _build(mrl, mrl_dim=4)
        info = json.loads(
            subprocess.run(
                [str(CLI), "inspect", "--json", mrl], capture_output=True, text=True, cwd=REPO
            ).stdout
        )
        assert info["manifest"]["full_dim"] == 8 and info["manifest"]["embedding_dim"] == 4
        rc, out, err = _ask(mrl, env_fake=True)
        assert rc == 0, f"--mrl-dim must be passed for truncated corpora: {err}"
        assert "urna://" in out
        print("case 3 (mrl corpus gets --mrl-dim, dim gate passes): OK")

        # sanity: sliced query really is the engine's truncation (scores align)
        emb = mr.create_embedder("fake-test")
        q = mr.slice_renorm(emb.embed_texts(["fake chunk number 2"]), 4)[0]
        db = urna.open(mrl)
        hits = db.search([float(x) for x in q], k=1)
        assert hits[0].score > 0.999
        print("case 4 (slice_renorm query == stored truncation): OK")

        # the search-text embedder slices the same way (one geometry for both
        # query paths), and refuses an out-of-range dim.
        import embed_query

        full = [float(x) for x in emb.embed_texts(["fake chunk number 2"])[0]]
        st_q = embed_query.slice_renorm(full, 4)
        assert len(st_q) == 4
        assert all(abs(a - float(b)) < 1e-6 for a, b in zip(st_q, q, strict=True))
        assert db.search(st_q, k=1)[0].score > 0.999
        proc = subprocess.run(
            [sys.executable, str(REPO / "python" / "embed_query.py"), "--help"],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 0 and "--mrl-dim" in proc.stdout, proc.stdout
        print("case 5 (embed_query.slice_renorm == registry slice_renorm): OK")

        _st_fallback_cases(tmp)

    print("all query embedder routing tests passed")


ST_MODEL = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"
MODEL_SCRIPT = REPO / "python" / "forge" / "embed_query_model.py"


def _has_st(py: str) -> bool:
    probe = "import importlib.util as u; print(u.find_spec('sentence_transformers') is not None)"
    p = subprocess.run([py, "-c", probe], capture_output=True, text=True)
    return p.stdout.strip() == "True"


def _model_embed(py: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run([py, str(MODEL_SCRIPT), *args], capture_output=True, text=True, cwd=REPO)


def _st_fallback_cases(tmp: str) -> None:
    """A model no preset names goes to embed_query.py, so a corpus built with a
    sentence-transformers model outside the registry is askable. The st cases
    need an interpreter with sentence-transformers and the model cached
    (URNA_ST_PYTHON picks one); without them they skip."""
    if not _has_st(sys.executable):
        p = _model_embed(sys.executable, "acme/unknown-model", "q")
        assert p.returncode == 4 and "sentence-transformers" in p.stderr, p.stderr
        print("case 6 (no preset, no sentence-transformers: exit 4 naming it): OK")
    st_py = os.environ.get("URNA_ST_PYTHON", sys.executable)
    if not _has_st(st_py):
        print("cases 7-9 skipped: no interpreter with sentence-transformers (URNA_ST_PYTHON)")
        return
    probe = _model_embed(st_py, ST_MODEL, "probe")
    if probe.returncode != 0:
        print(f"cases 7-9 skipped: {ST_MODEL} is not in the local cache")
        return

    texts = ["o consumidor tem direito à troca", "receita de bolo de cenoura", "pix sem tarifa"]
    payloads = [json.loads(_model_embed(st_py, ST_MODEL, t).stdout) for t in texts]
    corpus = str(Path(tmp) / "st.urna")
    urna.build(
        corpus,
        ST_MODEL,
        payloads[0]["embedding_dim"],
        "t/1",
        payloads[0]["model_hash"],
        [
            {
                "canonical_text": t,
                "source_uri": "test://st",
                "byte_start": 0,
                "byte_end": len(t),
                "embedding": pl["vector"],
            }
            for t, pl in zip(texts, payloads, strict=True)
        ],
        preset="exact",
        reproducible=True,
    )
    env = dict(os.environ, URNA_PYTHON=st_py)
    p = subprocess.run(
        [str(CLI), "ask", corpus, "direito do consumidor", "-k", "1"],
        capture_output=True,
        text=True,
        cwd=REPO,
        env=env,
    )
    assert p.returncode == 0, p.stderr
    assert "o consumidor tem direito" in p.stdout, p.stdout
    print("case 7 (st model outside the registry: ask routed, gate passes, right hit): OK")

    env = {k: v for k, v in os.environ.items() if k != "URNA_ALLOW_DOWNLOAD"}
    p = subprocess.run(
        [st_py, str(MODEL_SCRIPT), "acme/not-a-cached-model", "q"],
        capture_output=True,
        text=True,
        cwd=REPO,
        env=env,
    )
    assert p.returncode != 0 and "no registry preset" not in p.stderr, p.stderr
    assert "LocalEntryNotFoundError" in p.stderr, "the lookup must stay offline: " + p.stderr
    print("case 8 (unknown model reaches embed_query.py and fails offline, no hub call): OK")

    p = _model_embed(st_py, "--mrl-dim", "128", ST_MODEL, "q")
    assert p.returncode == 0, p.stderr
    out = json.loads(p.stdout)
    assert out["embedding_dim"] == 128 and len(out["vector"]) == 128
    assert out["model_hash"] == payloads[0]["model_hash"]
    print("case 9 (--mrl-dim passes through the fallback, model_hash unchanged): OK")


if __name__ == "__main__":
    main()
