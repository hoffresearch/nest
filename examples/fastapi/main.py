"""fastapi + nestdb: offline cited answers (issue #75).

loads the corpus once at startup and serves `POST /ask` with a text query;
the query is embedded OFFLINE with the potion table bundled in the wheel
(nestdb[embed]), and every hit returns the tier-1 canonical text plus its
nest:// citation. no network at runtime by construction.

setup:  pip install fastapi uvicorn "nestdb[embed]"
run:    uvicorn main:app --port 8000
try:    curl -s localhost:8000/ask -H 'content-type: application/json' \
          -d '{"query": "vector search on the edge", "k": 2}'

the demo corpus builds itself on first run (a handful of sentences embedded
with potion, reproducible=True). point NEST_FILE at a real potion-built
corpus for anything serious.
"""

from __future__ import annotations

import os
from contextlib import asynccontextmanager
from pathlib import Path

import nest
from fastapi import FastAPI
from nest.embed_potion import potion_embedder
from pydantic import BaseModel

NEST_FILE = Path(os.environ.get("NEST_FILE", "demo_fastapi.nest"))

DOCS = [
    "nest is a single-file vector database that works fully offline.",
    "vector search on the edge needs no server and no api key.",
    "every search hit carries a content-addressable citation.",
    "the potion static table embeds queries without a gpu or network.",
    "a .nest file bundles chunks, embeddings, and indices in one artifact.",
    "fastapi serves the corpus with a cited answer endpoint.",
]

emb = potion_embedder()


def _bootstrap_corpus(path: Path) -> None:
    """build the tiny demo corpus once, byte-identical across machines."""
    chunks = []
    for i, text in enumerate(DOCS):
        chunks.append(
            {
                "canonical_text": text,
                "source_uri": f"example://fastapi/{i}",
                "byte_start": 0,
                "byte_end": len(text.encode()),
                "embedding": emb.embed_texts([text])[0],
            }
        )
    nest.build(
        str(path),
        emb.embedding_model,
        emb.embedding_dim,
        "fastapi-example-1",
        emb.model_hash(),
        chunks,
        reproducible=True,
    )


@asynccontextmanager
async def lifespan(app: FastAPI):
    if not NEST_FILE.exists():
        _bootstrap_corpus(NEST_FILE)
    app.state.db = nest.open(str(NEST_FILE))
    app.state.db.validate()
    yield


app = FastAPI(lifespan=lifespan)


class Ask(BaseModel):
    query: str
    k: int = 3


@app.post("/ask")
def ask(req: Ask):
    qvec = emb.embed_texts([req.query])[0]
    hits = app.state.db.retrieve(qvec, req.k)
    return {
        "hits": [
            {
                "text": h.text,
                "score": h.score,
                "citation_id": h.citation_id,
                "source_uri": h.source_uri,
            }
            for h in hits
        ]
    }


@app.get("/health")
def health():
    return {"corpus": str(NEST_FILE), "file_hash": app.state.db.file_hash}
