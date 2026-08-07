"""Build a searchable `.nest` corpus from an image directory or from PDFs.

    images (or rendered pdf pages)
      -> one fixed canvas
      -> one AV1 stream                      (optional, --no-compress skips it)
      -> vision embeddings of the encoded frames
      -> .nest, one chunk per image or page

The index describes what a reader can actually get back, so when the corpus
is compressed the vectors are taken from the DECODED frames, not from the
source pixels. Building from the source pixels and shipping the compressed
stream would report a quality the corpus does not have.

A corpus is a directory: `corpus.nest` next to `corpus.media/`. Frame URIs
are relative to that pair, so the corpus can be copied elsewhere and still
resolve. `corpus.manifest.json` records what went in, for audit and for
`nest_image_eval.py`.

Usage:
    python/tools/nest_build_image_corpus.py \\
        --input-dir dat/demo/derm/ph2/images --dataset ph2 \\
        --output tmp/ph2/ph2.nest --labels tmp/ph2/labels.json
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from contextlib import ExitStack
from pathlib import Path
from tempfile import TemporaryDirectory

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from builder import BuildConfig, ChunkSpec, Pipeline
from forge import embed_image, image_items, image_media

CHUNKER_VERSION = "image-v1"


def _embed_compressed(embedder, media_path: Path, canvas, expected: int) -> np.ndarray:
    batches = [
        embedder.embed_arrays(batch)
        for batch in image_media.decode_frames(
            media_path, canvas, batch_size=max(1, embedder.batch_size)
        )
    ]
    vectors = np.vstack(batches) if batches else np.zeros((0, embedder.dim), dtype=np.float32)
    if len(vectors) != expected:
        raise RuntimeError(f"decoded {len(vectors)} frames for {expected} items")
    return vectors


def build_corpus(
    input_dir: Path,
    output_path: Path,
    dataset_name: str,
    *,
    embedder: embed_image.ImageEmbedder,
    is_pdf: bool = False,
    compress: bool = True,
    labels: dict[str, str] | None = None,
    sample: int | None = None,
    seed: int = 42,
    width: int = 1024,
    crf: int = 35,
    speed: int = 8,
    all_intra: bool = False,
    scratch_db: str | None = None,
    preset: str = "compressed",
) -> dict:
    input_dir, output_path = Path(input_dir), Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with ExitStack() as stack:
        if is_pdf:
            # the rendered pages must outlive the encode and the embed, so the
            # temp dir is bound to the whole build, not to the render call.
            tmp_dir = Path(stack.enter_context(TemporaryDirectory(prefix="nest-pdf-")))
            items = image_items.render_pdf_pages(input_dir, tmp_dir)
        else:
            items = image_items.collect_images(input_dir, labels)
        items = image_items.subsample(items, sample, seed)
        if not items:
            raise RuntimeError(f"no input found under {input_dir}")

        render_paths = [Path(item.render_path) for item in items]
        media_info = None
        if compress:
            canvas = image_media.canvas_size(render_paths, width)
            media_dir = image_media.media_dir_for(output_path)
            media_name = f"{dataset_name}-av1.mp4"
            media_path = media_dir / media_name
            media_info = image_media.encode_av1(
                render_paths,
                media_path,
                canvas=canvas,
                crf=crf,
                preset=speed,
                keyint=1 if all_intra else None,
            )
            uris = [f"media://{media_name}#frame={i}" for i in range(len(items))]
            embeddings = _embed_compressed(embedder, media_path, canvas, len(items))
        else:
            uris = [Path(p).resolve().as_uri() for p in render_paths]
            embeddings = embedder.embed_paths(render_paths)

        cfg = BuildConfig(
            output_path=str(output_path),
            embedding_model=embedder.model_id,
            embedding_dim=embedder.dim,
            chunker_version=CHUNKER_VERSION,
            model_hash=embedder.model_hash,
            title=f"{dataset_name} image corpus",
            version="0.1.0",
            description=f"vision embeddings for {dataset_name}",
            preset=preset,
        )
        specs = [
            ChunkSpec(
                canonical_text=item.canonical_text(),
                source_uri=uri,
                byte_start=item.ordinal,
                byte_end=item.ordinal + 1,
            )
            for item, uri in zip(items, uris, strict=True)
        ]

        # keyed by chunk_id, never by position: Pipeline hands the embedder
        # only the chunks the scratch cache missed, so a positional lookup
        # returns a DIFFERENT image's vector on any partially warm run.
        by_id = {
            spec.chunk_id(CHUNKER_VERSION): vec.tolist()
            for spec, vec in zip(specs, embeddings, strict=True)
        }
        pipe = Pipeline(
            cfg,
            embedder=lambda missed: [by_id[s.chunk_id(CHUNKER_VERSION)] for s in missed],
            scratch_db=scratch_db,
        )
        pipe.add_many(specs)
        provenance = {
            "dataset": dataset_name,
            "n_items": len(items),
            "compressed": compress,
            "media": media_info,
            "sample": {"size": sample, "seed": seed} if sample else None,
            "input_kind": "pdf" if is_pdf else "images",
        }
        pipe.emit(provenance=provenance)
        pipe.close()

    manifest = {
        "dataset": dataset_name,
        "nest": output_path.name,
        "compressed": compress,
        # the durable way back to a query image. for pdfs the rendered pages
        # are build-time temporaries, so `input_dir` + `origin` + `page` is
        # what the eval harness re-renders from.
        "input_dir": str(Path(input_dir).resolve()),
        "input_kind": "pdf" if is_pdf else "images",
        "media": media_info,
        "model": {"id": embedder.model_id, "dim": embedder.dim, "hash": embedder.model_hash},
        "items": [
            {
                "ordinal": item.ordinal,
                "origin": item.origin,
                "label": item.label,
                "page": item.page,
                "source_uri": uri,
                # absolute path of what was embedded, for the eval harness;
                # never part of the corpus itself.
                "render_path": item.render_path,
            }
            for item, uri in zip(items, uris, strict=True)
        ],
    }
    manifest_path = output_path.with_suffix(".manifest.json")
    manifest_path.write_text(json.dumps(manifest, indent=2))
    return {
        "dataset": dataset_name,
        "nest": str(output_path),
        "manifest": str(manifest_path),
        "n_items": len(items),
        "compressed": compress,
        "media": media_info,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--pdf", action="store_true", help="render pdf pages as the images")
    parser.add_argument("--no-compress", action="store_true", help="build the control index")
    parser.add_argument("--model", default="hf-hub:redlessone/DermLIP_ViT-B-16")
    parser.add_argument("--pretrained", help="required for bare architecture names")
    parser.add_argument("--device")
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--sample", type=int)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--width", type=int, default=1024)
    parser.add_argument("--crf", type=int, default=35)
    parser.add_argument("--speed", type=int, default=8, help="svt-av1 preset, 0 best to 13 fast")
    parser.add_argument(
        "--all-intra",
        action="store_true",
        help="every frame a keyframe; try it when the images are unrelated to each other",
    )
    parser.add_argument("--preset", default="compressed", help="nest encoding preset")
    parser.add_argument("--scratch-db")
    parser.add_argument("--labels", type=Path, help="json map or csv of image id to label")
    args = parser.parse_args()

    embedder = embed_image.ImageEmbedder(
        model_id=args.model,
        pretrained=args.pretrained,
        device=args.device,
        batch_size=args.batch_size,
    )
    print(
        json.dumps(
            build_corpus(
                input_dir=args.input_dir,
                output_path=args.output,
                dataset_name=args.dataset,
                embedder=embedder,
                is_pdf=args.pdf,
                compress=not args.no_compress,
                labels=image_items.load_labels(args.labels),
                sample=args.sample,
                seed=args.seed,
                width=args.width,
                crf=args.crf,
                speed=args.speed,
                all_intra=args.all_intra,
                scratch_db=args.scratch_db,
                preset=args.preset,
            ),
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
