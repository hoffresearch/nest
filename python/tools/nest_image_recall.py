"""Measure Recall@K for an image corpus built with nest_build_image_corpus.

The reference index contains embeddings extracted from the original images.
The query set is a sample of original images. We search the built .nest
(which may contain embeddings from compressed AV1 frames) and check whether
the matching frame is in the top-k.

Usage:
    .venv/bin/python python/tools/nest_image_recall.py \\
        --index tmp/glyfos.nest \\
        --input-dir /Volumes/HOFF/dat/glyfos-graph/multiscript_glyph_images_v1 \\
        --dataset glyfos \\
        --sample 50
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import nest
import numpy as np
from forge import embed_image


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--index", required=True, type=Path)
    parser.add_argument("--input-dir", required=True, type=Path)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--sample", type=int, default=100)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--model", default="hf-hub:redlessone/DermLIP_ViT-B-16")
    parser.add_argument("--pretrained")
    parser.add_argument("--device")
    parser.add_argument("-k", nargs="+", type=int, default=[1, 5, 10])
    args = parser.parse_args()

    embedder = embed_image.ImageEmbedder(
        model_id=args.model, pretrained=args.pretrained, device=args.device
    )
    images = embed_image.list_images(args.input_dir)
    if not images:
        raise RuntimeError(f"no images in {args.input_dir}")

    rng = np.random.default_rng(args.seed)
    n = min(args.sample, len(images))
    sample_idx = rng.choice(len(images), size=n, replace=False)
    sample_idx.sort()
    sample_images = [images[i] for i in sample_idx]

    db = nest.open(str(args.index))
    hits_at = {k: 0 for k in args.k}
    max_k = max(args.k)

    # The chunks in the index are in the same sorted order as `images`.
    # expected_frame is the global ordinal in that order.
    for img_path in sample_images:
        expected_frame = images.index(img_path)
        qvec = embedder.embed_single(img_path).tolist()
        results = db.search(qvec, max_k)
        returned_frames = [r.offset_start for r in results]
        for k in args.k:
            if expected_frame in returned_frames[:k]:
                hits_at[k] += 1

    recall = {f"recall@{k}": hits_at[k] / len(sample_images) for k in args.k}
    output = {
        "dataset": args.dataset,
        "sample_size": len(sample_images),
        "seed": args.seed,
        **recall,
    }
    print(json.dumps(output, indent=2))

    out_file = Path("tmp") / "embed-datasets-image-derm" / f"recall_{args.dataset}.json"
    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text(json.dumps(output, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
