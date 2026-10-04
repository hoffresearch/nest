---
project: urna
audience: integrators
status: active
last-updated: 2026-10-04
domain: examples
---

# jupyter + urna example

`urna_minimal.ipynb` builds a tiny potion-embedded corpus, validates it,
and runs one cited retrieve, all offline. setup:

```
pip install "urna[embed]" jupyter
jupyter notebook urna_minimal.ipynb
```

Python 3.12 or newer (the wheel is abi3, cp312). The corpus must be built with the embedder the notebook queries with, potion-base-8M here: `retrieve` passes `expected_model_hash=emb.model_hash()`, so a corpus built with another model is refused (`ValueError: model_hash mismatch`) instead of answering with wrong hits, and the query text as `query_text`, so a corpus with a BM25 section takes the hybrid route.
