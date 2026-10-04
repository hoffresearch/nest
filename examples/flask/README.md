---
project: urna
audience: integrators
status: active
last-updated: 2026-10-04
domain: examples
---

# flask + urna example

offline cited answers from a single-file corpus, minimal flask flavor.
nothing here touches the network after `pip install`.

Python 3.12 or newer (the wheel is abi3, cp312). The corpus must be built with the embedder the app queries with, potion-base-8M here: `retrieve` passes `expected_model_hash=emb.model_hash()`, so a corpus built with another model is refused (`ValueError: model_hash mismatch`) instead of answering with wrong hits, and the query text as `query_text`, so a corpus with a BM25 section takes the hybrid route.

## setup

```
pip install flask "urna[embed]"
```

## run

```
flask --app app run --port 8000
```

## try

```
curl -s localhost:8000/ask -H 'content-type: application/json' \
  -d '{"query": "vector search on the edge", "k": 2}'
```

see `../fastapi/README.md` for the corpus bootstrap notes; the flow is the
same (`URNA_FILE` points at your corpus, the demo builds itself once).
