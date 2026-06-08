"""group recall@k UNFILTERED vs metadata-FILTERED on a .nest carrying a 0x17
meta_index. it answers one concrete question with measured numbers: how much
does scoping the exact cosine to a (field, value) subset change retrieval and
cost?

  unfiltered: query = a doc's body head, ranked over the WHOLE corpus
              (db.search). recall@k = fraction of the doc's group siblings
              (docs sharing its field value) surfaced in the top k.
  filtered  : same query, ranked only over the chunks sharing that doc's field
              value (db.search_filtered). the candidate set IS the group, so
              within-group retrieval becomes exact and complete.

prints ONLY aggregate recall, candidate-set sizes, and timings; never content.
the field is whatever was indexed at build time -- no market rule lives in nest.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))  # python/
import nest  # noqa: E402
from forge.embed_potion import potion_embedder  # noqa: E402


def load(path: str) -> list[dict]:
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--nest", required=True)
    ap.add_argument("--sidecar", required=True)
    ap.add_argument("--field", required=True, help="meta_index field to filter on")
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--queries", type=int, default=2000)
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    docs = load(args.sidecar)
    emb = potion_embedder(args.model_dir) if args.model_dir else potion_embedder()
    db = nest.open(args.nest)
    print(
        f"# nest={os.path.basename(args.nest)} n_emb={db.n_embeddings} dtype={db.dtype} "
        f"has_meta_index={db.has_meta_index} fields={db.meta_index_fields()}",
        file=sys.stderr,
    )

    def val(d: dict):
        v = (d.get("meta") or {}).get(args.field)
        return None if v is None else str(v)

    def qtext(d: dict):
        q = d.get("query_text")
        return q.strip() if isinstance(q, str) and q.strip() else None

    # group docs by field value; count chunks per value (the filtered candidate
    # set size = the meta_index posting length for that value).
    groups: dict[str, list] = {}
    chunks_per_value: dict[str, int] = {}
    for d in docs:
        v = val(d)
        if v is None:
            continue
        chunks_per_value[v] = chunks_per_value.get(v, 0) + int(d.get("n_chunks") or 0)
        if qtext(d):
            groups.setdefault(v, []).append(d)
    multi = {v: m for v, m in groups.items() if len(m) >= 2}
    pool = [d for m in multi.values() for d in m]
    rng = random.Random(args.seed)
    sample = pool if len(pool) <= args.queries else rng.sample(pool, args.queries)

    def recall_over(hits, d: dict, sibs: set) -> float:
        ranked: list[str] = []
        for h in hits:
            if h.source_uri == d["source_uri"] or h.source_uri in ranked:
                continue  # drop the query note's own chunks; dedup hits to notes
            ranked.append(h.source_uri)
            if len(ranked) >= args.k:
                break
        return len(set(ranked) & sibs) / min(args.k, len(sibs))

    qvs = emb.embed_texts([qtext(d) for d in sample])
    g_rec = f_rec = g_t = f_t = 0.0
    g_cand = f_cand = n = 0
    for d, qv in zip(sample, qvs, strict=True):
        v = val(d)
        sibs = {o["source_uri"] for o in multi[v] if o["source_uri"] != d["source_uri"]}
        if not sibs:
            continue
        t0 = time.perf_counter()
        gh = db.search(qv, args.k + 40)
        g_t += time.perf_counter() - t0
        t0 = time.perf_counter()
        fh = db.search_filtered(qv, args.field, v, args.k + 40)
        f_t += time.perf_counter() - t0
        g_rec += recall_over(gh, d, sibs)
        f_rec += recall_over(fh, d, sibs)
        g_cand += db.n_embeddings
        f_cand += chunks_per_value[v]
        n += 1

    out = {
        "field": args.field,
        "k": args.k,
        "queries": n,
        "unfiltered": {
            "group_recall_at_k": round(g_rec / n, 4),
            "mean_candidates": round(g_cand / n, 1),
            "mean_query_ms": round(1000 * g_t / n, 3),
        },
        "filtered": {
            "group_recall_at_k": round(f_rec / n, 4),
            "mean_candidates": round(f_cand / n, 1),
            "mean_query_ms": round(1000 * f_t / n, 3),
        },
    }
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
