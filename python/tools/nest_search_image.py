"""Search an image .nest corpus with a query image.

Usage:
    .venv/bin/python python/tools/nest_search_image.py \\
        --index tmp/glyfos.nest \\
        --query-image /Volumes/HOFF/dat/glyfos-graph/multiscript_glyph_images_v1/000001.png
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import nest
from forge import embed_image


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--index", required=True, type=Path)
    parser.add_argument("--query-image", required=True, type=Path)
    parser.add_argument("--model", default="hf-hub:redlessone/DermLIP_ViT-B-16")
    parser.add_argument("--pretrained")
    parser.add_argument("--device")
    parser.add_argument("-k", type=int, default=10)
    parser.add_argument("--ann", action="store_true")
    parser.add_argument("--ef", type=int, default=128)
    args = parser.parse_args()

    embedder = embed_image.ImageEmbedder(
        model_id=args.model, pretrained=args.pretrained, device=args.device
    )
    qvec = embedder.embed_single(args.query_image).tolist()

    db = nest.open(str(args.index))
    hits = db.search_ann(qvec, args.k, args.ef) if args.ann else db.search(qvec, args.k)

    results = []
    for h in hits:
        results.append(
            {
                "chunk_id": h.chunk_id,
                "score": float(h.score),
                "source_uri": h.source_uri,
                "offset_start": h.offset_start,
                "offset_end": h.offset_end,
                "citation_id": h.citation_id,
            }
        )
    print(json.dumps({"query_image": str(args.query_image), "hits": results}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
