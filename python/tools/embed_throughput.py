"""inference cost per query for each embedder: per-query latency (batch=1) and
batched throughput (queries/s), single-thread. times REAL note heads read from a
sidecar's query_text so token lengths are representative; prints ONLY timings,
never text.

this is the half the recall bench omitted: recall is model-determined (identical
to fastembed-rs for the onnx models), but the offline-no-socket thesis pays the
per-query inference cost at serve time, and the recall winner is usually the
slowest. measured here so the embedder tradeoff is complete.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))  # python/


def texts_from(sidecar: str, n: int, chars: int) -> list[str]:
    out: list[str] = []
    with open(sidecar) as f:
        for line in f:
            if not line.strip():
                continue
            q = json.loads(line).get("query_text")
            if isinstance(q, str) and q.strip():
                out.append(q[:chars])
                if len(out) >= n:
                    break
    return out


def potion_fn(model_dir):
    from forge.embed_potion import PotionEmbedder

    e = PotionEmbedder(model_dir) if model_dir else PotionEmbedder()
    label = "potion:" + (os.path.basename(model_dir) if model_dir else "base-8M")
    return label, e.embedding_dim, lambda ts: e.embed_texts(ts)


def fastembed_fn(spec):
    from fastembed import TextEmbedding

    name, _, pfx = spec.partition(":")
    m = TextEmbedding(model_name=name)

    def run(ts):
        return list(m.embed([pfx + t for t in ts] if pfx else ts))

    dim = len(list(m.embed(["x"]))[0])
    return "fastembed:" + name.split("/")[-1], dim, run


def bench(label: str, dim: int, run, texts: list[str]) -> None:
    run(texts[:8])  # warmup (pays model load / first onnx session)
    t0 = time.perf_counter()
    run(texts)
    batch_s = time.perf_counter() - t0
    sub = texts[:64]
    t0 = time.perf_counter()
    for t in sub:
        run([t])
    single_ms = 1000 * (time.perf_counter() - t0) / len(sub)
    qps = len(texts) / batch_s if batch_s > 0 else float("nan")
    print(f"{label:42}{dim:>6}{single_ms:>12.3f}{qps:>12.1f}", flush=True)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sidecar", required=True)
    ap.add_argument("--n", type=int, default=512)
    ap.add_argument("--chars", type=int, default=512)
    ap.add_argument("--potion", action="append", default=[])
    ap.add_argument("--fastembed", action="append", default=[])
    a = ap.parse_args()

    texts = texts_from(a.sidecar, a.n, a.chars)
    print(f"# n_texts={len(texts)} chars<={a.chars} single-thread", file=sys.stderr)
    print(f"\n{'embedder':42}{'dim':>6}{'ms/query':>12}{'queries/s':>12}", flush=True)
    for d in a.potion:
        try:
            bench(*potion_fn(d or None), texts)
        except Exception as e:  # noqa: BLE001 (a bench must not die on one model)
            print(f"{'potion:' + (d or 'base-8M'):42}  FAILED: {type(e).__name__}: {e}", flush=True)
    for s in a.fastembed:
        try:
            bench(*fastembed_fn(s), texts)
        except Exception as e:  # noqa: BLE001
            print(f"{'fastembed:' + s:42}  FAILED: {type(e).__name__}: {e}", flush=True)


if __name__ == "__main__":
    main()
