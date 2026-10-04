---
project: urna
audience: integrators
status: active
last-updated: 2026-10-04
domain: examples
---

# fastapi + urna example

offline cited answers from a single-file corpus. the query is embedded with
the potion table bundled in the `urna` wheel, so nothing here touches the
network after `pip install`.

Python 3.12 or newer (the wheel is abi3, cp312). The corpus must be built with the embedder the app queries with, potion-base-8M here: `retrieve` passes `expected_model_hash=emb.model_hash()`, so a corpus built with another model is refused (`ValueError: model_hash mismatch`) instead of answering with wrong hits, and the query text as `query_text`, so a corpus with a BM25 section takes the hybrid route.

## setup

```
pip install fastapi uvicorn "urna[embed]"
```

## run

```
uvicorn main:app --port 8000
```

the demo corpus (`demo_fastapi.urna`) builds itself on first run with
`reproducible=True`, so two machines produce the same `file_hash`. point
`URNA_FILE` at a real potion-built corpus for real data.

## try

```
curl -s localhost:8000/ask -H 'content-type: application/json' \
  -d '{"query": "vector search on the edge", "k": 2}'
```

each hit returns the stored canonical text, the exact-cosine score, the
source uri, and the `urna://content_hash/chunk_id` citation.
