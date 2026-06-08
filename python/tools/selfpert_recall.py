"""self-perturbation recall@k: the weak ruler, measured on whatever corpus.

replicates the measure_presets self-perturbation query (a stored chunk vector
plus tiny deterministic per-dim noise, re-l2-normalized) and asks the plain
question: does the perturbed query retrieve ITS OWN chunk in the top k? this is
the "find the near-identical point" task; on a real corpus it sits near 1.0,
which is exactly why it cannot stand in for real-query retrieval quality.

the f32 vectors are decoded out of a raw exact .nest, then a target .nest (the
same file, or a quantized sibling) is searched, so an exact-vs-quantized gap is
measured on identical queries. own-chunk identity is the (source_uri,
offset_start) pair, which is unique per chunk. prints only aggregate recall.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)  # python/tools (for _baseline_decoder)
sys.path.insert(0, os.path.join(HERE, ".."))  # python/
import nest  # noqa: E402
from _baseline_decoder import decode_baseline  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--source-nest", required=True, help="raw exact .nest to decode f32 vectors from")
    ap.add_argument("--target-nest", required=True, help=".nest to search (exact or quantized sibling)")
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--queries", type=int, default=3000)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    chunks, meta = decode_baseline(Path(args.source_nest))
    n = len(chunks)
    dim = meta["embedding_dim"]
    rng = random.Random(args.seed)
    idxs = list(range(n)) if n <= args.queries else rng.sample(range(n), args.queries)

    db = nest.open(args.target_nest)
    route = "ann" if db.has_ann else "exact"
    hits = 0
    for i in idxs:
        c = chunks[i]
        emb = list(c["embedding"])
        for j in range(dim):
            emb[j] += ((j * 7 + i) % 17 - 8) * 1e-5
        nrm = sum(x * x for x in emb) ** 0.5 or 1.0
        q = [x / nrm for x in emb]
        if db.embedding_dim < len(q):  # matryoshka target: truncate+renorm the query
            q = q[: db.embedding_dim]
            n2 = sum(x * x for x in q) ** 0.5 or 1.0
            q = [x / n2 for x in q]
        res = db.search_ann(q, args.k, max(args.k * 4, 64)) if db.has_ann else db.search(q, args.k)
        if any(h.source_uri == c["source_uri"] and h.offset_start == c["byte_start"] for h in res):
            hits += 1

    print(
        json.dumps(
            {
                "source": os.path.basename(args.source_nest),
                "target": os.path.basename(args.target_nest),
                "route": route,
                "dtype": db.dtype,
                "n_chunks": n,
                "k": args.k,
                "queries": len(idxs),
                "self_perturbation_recall_at_k": round(hits / len(idxs), 4) if idxs else 0.0,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
