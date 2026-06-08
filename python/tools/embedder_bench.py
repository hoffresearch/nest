"""note-level group recall@k across embedders, on ONE fixed subset, so static
(model2vec) and onnx (fastembed) embedders sit in a single comparable table.

each note is embedded once (the head of its body); the relevant set for a note
is the OTHER notes sharing its group id (e.g. patient_file_id). recall@k =
fraction of a note's group siblings that land in its top k by cosine (self
excluded). this decouples the embedder comparison from the .nest build, so it is
a clean A/B of the embedder alone. prints only aggregate recall, never content.

embedders are passed as repeatable specs:
  --potion DIR            a model2vec table dir ("" = vendored potion-base-8M)
  --fastembed NAME[:PFX]  a fastembed/onnx model, optional query prefix (e5 wants "query: ")
"""

from __future__ import annotations

import argparse
import collections
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))  # python/
from forge.zip_source import stream_zip_json  # noqa: E402


def _l2(m: np.ndarray) -> np.ndarray:
    n = np.linalg.norm(m, axis=1, keepdims=True)
    n[n == 0] = 1.0
    return m / n


def _potion_embed(model_dir, texts):
    from forge.embed_potion import PotionEmbedder

    e = PotionEmbedder(model_dir) if model_dir else PotionEmbedder()
    return np.asarray(e.embed_texts(texts), dtype=np.float32)


def _fastembed_embed(name, prefix, texts):
    from fastembed import TextEmbedding

    m = TextEmbedding(model_name=name)
    src = [prefix + t for t in texts] if prefix else texts
    return np.asarray(list(m.embed(src)), dtype=np.float32)


def group_recall(vecs: np.ndarray, groups: list, k: int) -> tuple[float, int]:
    v = _l2(vecs.astype(np.float32))
    sims = v @ v.T
    np.fill_diagonal(sims, -1.0)  # never retrieve self
    n = len(groups)
    gidx: dict = collections.defaultdict(list)
    for i, g in enumerate(groups):
        gidx[g].append(i)
    kk = min(k, n - 1)
    topk = np.argpartition(-sims, kth=kk, axis=1)[:, :k]
    recs = []
    for i in range(n):
        gold = set(gidx[groups[i]]) - {i}
        if gold:
            recs.append(len(set(topk[i].tolist()) & gold) / min(k, len(gold)))
    return (float(np.mean(recs)) if recs else 0.0), len(recs)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--zip", required=True)
    ap.add_argument("--names-file", required=True)
    ap.add_argument("--text-field", default="log.decrypted_notes")
    ap.add_argument("--group-field", default="log.patient_file_id")
    ap.add_argument("--query-chars", type=int, default=512)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--potion", action="append", default=[])
    ap.add_argument("--fastembed", action="append", default=[])
    args = ap.parse_args()

    with open(args.names_file) as f:
        names = [ln.strip() for ln in f if ln.strip()]
    docs = list(
        stream_zip_json(
            args.zip,
            text_fields=[args.text_field],
            meta_fields=[args.group_field],
            names=names,
        )
    )
    texts = [d.text[: args.query_chars] for d in docs]
    groups = [d.meta.get(args.group_field) for d in docs]
    multi = sum(1 for g, c in collections.Counter(groups).items() if c >= 2)
    print(f"# notes={len(docs)}  multi-note groups={multi}  k={args.k}", file=sys.stderr)

    print(f"\n{'embedder':48}{'dim':>6}{'group@k':>9}{'queries':>9}", flush=True)

    def emit(nm: str, v: np.ndarray) -> None:
        r, q = group_recall(v, groups, args.k)
        print(f"{nm:48}{v.shape[1]:>6}{r:>9.4f}{q:>9}", flush=True)

    for d in args.potion:
        label = f"potion:{os.path.basename(d) or 'base-8M'}"
        try:
            emit(label, _potion_embed(d or None, texts))
        except Exception as e:  # noqa: BLE001  (a benchmark must not die on one model)
            print(f"{label:48}  FAILED: {type(e).__name__}: {e}", flush=True)
    for spec in args.fastembed:
        name, _, prefix = spec.partition(":")
        label = f"fastembed:{name.split('/')[-1]}"
        try:
            emit(label, _fastembed_embed(name, prefix, texts))
        except Exception as e:  # noqa: BLE001
            print(f"{label:48}  FAILED: {type(e).__name__}: {e}", flush=True)


if __name__ == "__main__":
    main()
