"""real-query recall@k over a .nest built from streamed json docs.

the self-perturbation ruler queries with a corpus vector plus noise, so it only
measures rank-stability under quantization. this harness instead scores
retrieval with REAL queries drawn from the data's own structure:

  title->body : the query is a document's human-written title; the relevant doc
                is that document's body (indexed separately, so the query text is
                NOT inside the indexed chunk). recall@k = the document's own note
                surfaces in the top k.
  group       : the query is one document's title; the relevant set is the OTHER
                documents sharing a group id (e.g. patient_file_id). recall@k =
                fraction of the group's other docs surfaced in the top k.

queries and labels come from the sidecar jsonl written by build_from_zip.py.
prints ONLY aggregate recall, never document content.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))  # python/
import nest  # noqa: E402
from forge.embed_potion import potion_embedder  # noqa: E402


def load_sidecar(path: str) -> list[dict]:
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def _fit(qvec: list[float], dim: int) -> list[float]:
    """truncate+renormalize a query to a matryoshka index's prefix dim, matching
    the build-time truncate-then-renorm. a no-op when dims already match."""
    if len(qvec) <= dim:
        return qvec
    pre = qvec[:dim]
    n = sum(x * x for x in pre) ** 0.5 or 1.0
    return [x / n for x in pre]


def _search(db, qvec: list[float], k: int):
    q = _fit(qvec, db.embedding_dim)
    if db.has_ann:
        return db.search_ann(q, k, max(k * 4, 64))
    return db.search(q, k)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--nest", required=True)
    ap.add_argument("--sidecar", required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--queries", type=int, default=2000)
    ap.add_argument("--title-field", default="log.decrypted_title")
    ap.add_argument("--group-field", default="log.patient_file_id")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--model-dir", default=None, help="query embedder dir; MUST match the one the .nest was built with")
    args = ap.parse_args()

    docs = load_sidecar(args.sidecar)
    rng = random.Random(args.seed)
    emb = potion_embedder(args.model_dir) if args.model_dir else potion_embedder()
    db = nest.open(args.nest)
    route = "ann" if db.has_ann else "exact"

    def title_of(d: dict) -> str | None:
        t = (d.get("meta") or {}).get(args.title_field)
        return t.strip() if isinstance(t, str) and t.strip() else None

    def group_of(d: dict):
        return (d.get("meta") or {}).get(args.group_field)

    # ---- title -> body ----
    titled = [d for d in docs if title_of(d)]
    sample = titled if len(titled) <= args.queries else rng.sample(titled, args.queries)
    qvecs = emb.embed_texts([title_of(d) for d in sample])
    hits = 0
    for d, qv in zip(sample, qvecs, strict=True):
        uris = {h.source_uri for h in _search(db, qv, args.k)}
        hits += d["source_uri"] in uris
    tb_recall = hits / len(sample) if sample else 0.0

    def query_of(d: dict) -> str | None:
        q = d.get("query_text")
        return q.strip() if isinstance(q, str) and q.strip() else None

    # ---- body -> self (easy upper bound: query with the note's own body head) ----
    bodied = [d for d in docs if query_of(d)]
    bsample = bodied if len(bodied) <= args.queries else rng.sample(bodied, args.queries)
    bhits = 0
    for d, qv in zip(bsample, emb.embed_texts([query_of(d) for d in bsample]), strict=True):
        if d["source_uri"] in {h.source_uri for h in _search(db, qv, args.k)}:
            bhits += 1
    bs_recall = bhits / len(bsample) if bsample else 0.0

    # ---- group: a session's BODY query -> the OTHER sessions of the same patient ----
    groups: dict[object, list[dict]] = {}
    for d in docs:
        g = group_of(d)
        if g is not None and query_of(d):
            groups.setdefault(g, []).append(d)
    multi = {g: m for g, m in groups.items() if len(m) >= 2}
    pool = [d for m in multi.values() for d in m]
    gsample = pool if len(pool) <= args.queries else rng.sample(pool, args.queries)
    gvecs = emb.embed_texts([query_of(d) for d in gsample])
    grec, gcount = 0.0, 0
    for d, qv in zip(gsample, gvecs, strict=True):
        sibs = {o["source_uri"] for o in multi[group_of(d)] if o["source_uri"] != d["source_uri"]}
        if not sibs:
            continue
        ranked: list[str] = []
        for h in _search(db, qv, args.k + 40):
            if h.source_uri == d["source_uri"] or h.source_uri in ranked:
                continue  # dedup hits to notes and drop the query note's own chunks
            ranked.append(h.source_uri)
            if len(ranked) >= args.k:
                break
        grec += len(set(ranked) & sibs) / min(args.k, len(sibs))
        gcount += 1
    g_recall = grec / gcount if gcount else 0.0

    print(
        json.dumps(
            {
                "nest": os.path.basename(args.nest),
                "route": route,
                "dtype": db.dtype,
                "n_docs": len(docs),
                "k": args.k,
                "body_self": {"queries": len(bsample), "recall_at_k": round(bs_recall, 4)},
                "title_body": {"queries": len(sample), "recall_at_k": round(tb_recall, 4)},
                "group": {
                    "groups_multi": len(multi),
                    "queries": gcount,
                    "recall_at_k": round(g_recall, 4),
                },
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
