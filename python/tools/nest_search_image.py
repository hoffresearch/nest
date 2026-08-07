"""Search an image corpus with a query image.

Queries go through `retrieve`, so the embedder's `model_hash` is checked
against the manifest before anything is scored: an index built with one
checkpoint cannot be silently queried with another. Hits are resolved back
through the corpus manifest, so a result names the original file (and pdf
page), not only a frame ordinal.

`--save-frames` decodes the matched frames out of the corpus media and
writes them as PNGs. That is also the end-to-end check that the media and
the index still agree.

Usage:
    python/tools/nest_search_image.py --index tmp/ph2/ph2.nest \\
        --query-image lesion.jpg -k 5 --save-frames tmp/hits
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import nest
from forge import embed_image, image_media


def load_manifest(index_path: Path) -> dict | None:
    manifest_path = Path(index_path).with_suffix(".manifest.json")
    return json.loads(manifest_path.read_text()) if manifest_path.exists() else None


def save_frame(index_path: Path, manifest: dict, source_uri: str, out_dir: Path) -> str | None:
    """Decode one hit's frame out of the corpus media."""
    from PIL import Image

    media = manifest.get("media")
    if not media or not source_uri.startswith("media://"):
        return None
    name, ordinal = image_media.parse_media_uri(source_uri)
    media_path = image_media.media_dir_for(index_path) / name
    if not media_path.exists():
        raise FileNotFoundError(f"corpus media missing: {media_path}")
    frame = image_media.decode_frame(media_path, tuple(media["canvas"]), ordinal)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{ordinal:06d}.png"
    Image.fromarray(frame).save(out_path)
    return str(out_path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", required=True, type=Path)
    parser.add_argument("--query-image", required=True, type=Path)
    parser.add_argument("--model", default="hf-hub:redlessone/DermLIP_ViT-B-16")
    parser.add_argument("--pretrained")
    parser.add_argument("--device")
    parser.add_argument("-k", type=int, default=10)
    parser.add_argument("--save-frames", type=Path, help="write matched frames here as png")
    parser.add_argument(
        "--skip-model-check",
        action="store_true",
        help="query with a different embedder than the corpus was built with",
    )
    args = parser.parse_args()

    if not args.query_image.exists():
        raise FileNotFoundError(args.query_image)

    embedder = embed_image.ImageEmbedder(
        model_id=args.model, pretrained=args.pretrained, device=args.device
    )
    qvec = embedder.embed_one(args.query_image).tolist()

    db = nest.open(str(args.index))
    expected = None if args.skip_model_check else embedder.model_hash
    hits = db.retrieve(qvec, args.k, expected_model_hash=expected)

    manifest = load_manifest(args.index)
    by_ordinal = {i["ordinal"]: i for i in manifest["items"]} if manifest else {}

    results = []
    for hit in hits:
        item = by_ordinal.get(hit.offset_start, {})
        entry = {
            "score": round(float(hit.score), 6),
            "score_type": hit.score_type,
            "ordinal": hit.offset_start,
            "text": hit.text,
            "origin": item.get("origin"),
            "label": item.get("label") or None,
            "page": item.get("page"),
            "source_uri": hit.source_uri,
            "citation_id": hit.citation_id,
        }
        if args.save_frames and manifest:
            entry["frame_png"] = save_frame(args.index, manifest, hit.source_uri, args.save_frames)
        results.append({k: v for k, v in entry.items() if v is not None})

    print(json.dumps({"query": str(args.query_image), "hits": results}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
