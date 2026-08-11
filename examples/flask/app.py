"""flask + nestdb: offline cited answers (issue #75).

same shape as the fastapi example, minimal flask flavor: the corpus loads
once at import, `POST /ask` embeds the query OFFLINE with the potion table
bundled in the wheel (nestdb[embed]) and returns the tier-1 canonical text
plus nest:// citations. no network at runtime by construction.

setup:  pip install flask "nestdb[embed]"
run:    flask --app app run --port 8000
try:    curl -s localhost:8000/ask -H 'content-type: application/json' \
          -d '{"query": "vector search on the edge", "k": 2}'
"""

from __future__ import annotations

import os
from pathlib import Path

import nest
from flask import Flask, jsonify, request
from nest.embed_potion import potion_embedder

NEST_FILE = Path(os.environ.get("NEST_FILE", "demo_flask.nest"))

DOCS = [
    "nest is a single-file vector database that works fully offline.",
    "vector search on the edge needs no server and no api key.",
    "every search hit carries a content-addressable citation.",
    "the potion static table embeds queries without a gpu or network.",
    "a .nest file bundles chunks, embeddings, and indices in one artifact.",
    "flask serves the corpus with a cited answer endpoint.",
]

emb = potion_embedder()


def _bootstrap_corpus(path: Path) -> None:
    """build the tiny demo corpus once, byte-identical across machines."""
    chunks = [
        {
            "canonical_text": text,
            "source_uri": f"example://flask/{i}",
            "byte_start": 0,
            "byte_end": len(text.encode()),
            "embedding": emb.embed_texts([text])[0],
        }
        for i, text in enumerate(DOCS)
    ]
    nest.build(
        str(path),
        emb.embedding_model,
        emb.embedding_dim,
        "flask-example-1",
        emb.model_hash(),
        chunks,
        reproducible=True,
    )


if not NEST_FILE.exists():
    _bootstrap_corpus(NEST_FILE)
db = nest.open(str(NEST_FILE))
db.validate()

app = Flask(__name__)


@app.post("/ask")
def ask():
    body = request.get_json(force=True)
    qvec = emb.embed_texts([body["query"]])[0]
    hits = db.retrieve(qvec, int(body.get("k", 3)))
    return jsonify(
        hits=[
            {
                "text": h.text,
                "score": h.score,
                "citation_id": h.citation_id,
                "source_uri": h.source_uri,
            }
            for h in hits
        ]
    )


@app.get("/health")
def health():
    return jsonify(corpus=str(NEST_FILE), file_hash=db.file_hash)
