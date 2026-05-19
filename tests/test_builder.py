"""end-to-end test of the Python ingestion pipeline."""
import math
import os
import sys
import tempfile

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))

import nest
from builder import BuildConfig, EmbeddingCache, Pipeline, chunk_text


def _toy_embed(specs):
    """Toy embedder: hash-based deterministic 8-dim vector."""
    out = []
    for spec in specs:
        h = hash(spec.canonical_text)
        v = [((h >> (i * 8)) & 0xFF) / 255.0 for i in range(8)]
        n = math.sqrt(sum(x * x for x in v)) or 1.0
        out.append([x / n for x in v])
    return out


def test_chunk_text_byte_spans_round_trip():
    text = "Olá mundo. Esta é uma frase com acentuação e emoji 🚀."
    chunks = chunk_text(text, "doc.txt", max_chars=10, overlap=0)
    encoded = text.encode("utf-8")
    # Concatenated chunk bytes must equal the original encoding.
    rebuilt = b"".join(c.canonical_text.encode("utf-8") for c in chunks)
    assert rebuilt == encoded, (rebuilt, encoded)
    # Byte spans must point to the right place in the original encoding.
    for c in chunks:
        assert encoded[c.byte_start:c.byte_end] == c.canonical_text.encode("utf-8")


def test_pipeline_emits_validated_nest_file():
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "pipe.nest")
        cfg = BuildConfig(
            output_path=out,
            embedding_model="toy",
            embedding_dim=8,
            chunker_version="char/512",
            model_hash="sha256:" + "0" * 64,
            reproducible=True,
        )
        pipe = Pipeline(cfg, embedder=_toy_embed, scratch_db=os.path.join(d, "cache.db"))
        for source, text in [
            ("a.txt", "uma frase em português com acentuação"),
            ("b.txt", "outra frase, completamente diferente da primeira"),
        ]:
            pipe.add_many(chunk_text(text, source, max_chars=20))
        pipe.emit()
        pipe.close()

        db = nest.open(out)
        assert db.embedding_dim == 8
        assert db.n_embeddings >= 2
        hits = db.search([1.0] + [0.0] * 7, 1)
        assert len(hits) == 1
        assert hits[0].source_uri in ("a.txt", "b.txt")


def test_cache_skips_re_embedding_on_second_run():
    """If the scratch DB has the embedding, the embedder should not be
    invoked for that chunk on the second run."""
    with tempfile.TemporaryDirectory() as d:
        scratch = os.path.join(d, "cache.db")

        call_count = {"n": 0}

        def counting_embed(specs):
            call_count["n"] += len(specs)
            return _toy_embed(specs)

        text = "frase um. frase dois. frase tres."
        out_a = os.path.join(d, "a.nest")
        out_b = os.path.join(d, "b.nest")
        for out in (out_a, out_b):
            cfg = BuildConfig(
                output_path=out,
                embedding_model="toy",
                embedding_dim=8,
                chunker_version="char/8",
                model_hash="sha256:" + "0" * 64,
                reproducible=True,
            )
            pipe = Pipeline(cfg, embedder=counting_embed, scratch_db=scratch)
            pipe.add_many(chunk_text(text, "doc.txt", max_chars=8))
            pipe.emit()
            pipe.close()

        with open(out_a, "rb") as fa, open(out_b, "rb") as fb:
            assert fa.read() == fb.read(), "reproducible builds via cache must match"
        # First run embedded everything; second run should embed zero new chunks.
        # Hence call_count["n"] == n_chunks (from the first run only).
        n_chunks = len(chunk_text(text, "doc.txt", max_chars=8))
        assert call_count["n"] == n_chunks, (
            f"expected {n_chunks} embed calls, got {call_count['n']}"
        )


if __name__ == "__main__":
    test_chunk_text_byte_spans_round_trip()
    test_pipeline_emits_validated_nest_file()
    test_cache_skips_re_embedding_on_second_run()
    print("builder tests OK")
