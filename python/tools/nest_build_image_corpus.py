"""Build a searchable .nest corpus from image directories or PDFs.

Pipeline:
  1. Discover images (or render PDF pages to images).
  2. Optionally compress the image sequence to AV1 video or JPEG XL frames.
  3. Extract vision embeddings.
  4. Emit a .nest file where each image/page is one chunk.
  5. Measure Recall@k when labels are available.

Usage:
    python python/tools/nest_build_image_corpus.py \\
        --input-dir /Volumes/HOFF/dat/glyfos-graph/multiscript_glyph_images_v1 \\
        --dataset glyfos \\
        --output /Volumes/HOFF/dev/nest/tmp/glyfos.nest

For PDFs:
    python python/tools/nest_build_image_corpus.py \\
        --input-dir /Volumes/HOFF/dat/brasil/ambiental/aves \\
        --dataset aves-pdfs \\
        --pdf \\
        --output /Volumes/HOFF/dev/nest/tmp/aves.nest
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from builder import BuildConfig, Pipeline
from forge import embed_image


def render_pdfs(input_dir: Path, tmp_dir: Path, dpi: int = 150) -> list[tuple[Path, str, int]]:
    """Render every PDF page to an image. Returns list of (image_path, pdf_path, page_number)."""
    try:
        import fitz  # type: ignore[import-not-found] # PyMuPDF
    except ImportError as err:
        raise RuntimeError("PDF rendering requires PyMuPDF: pip install pymupdf") from err

    results = []
    for pdf_path in sorted(input_dir.rglob("*.pdf")):
        doc = fitz.open(str(pdf_path))
        for page_num in range(len(doc)):
            page = doc.load_page(page_num)
            pix = page.get_pixmap(dpi=dpi)
            out_name = f"{pdf_path.stem}_page{page_num:04d}.png"
            out_path = tmp_dir / out_name
            pix.save(str(out_path))
            results.append((out_path, str(pdf_path), page_num))
        doc.close()
    return results


def compress_av1(
    image_paths: list[Path],
    output_video: Path,
    *,
    crf: int = 35,
    preset: int = 8,
    scale: str = "1024:-2",
    fps: int = 1,
    lp: int = 2,
) -> dict:
    """Compress an ordered image list to an AV1 mp4 using ffmpeg.

    Returns a manifest dict with size and ratio metrics.
    """
    source_bytes = sum(p.stat().st_size for p in image_paths)
    filelist = output_video.with_suffix(".txt")
    with filelist.open("w") as f:
        for p in image_paths:
            f.write(f"file '{p}'\n")

    cmd = [
        "ffmpeg",
        "-y",
        "-hide_banner",
        "-loglevel",
        "warning",
        "-f",
        "concat",
        "-safe",
        "0",
        "-r",
        str(fps),
        "-i",
        str(filelist),
        "-vf",
        f"scale={scale}",
        "-c:v",
        "libsvtav1",
        "-crf",
        str(crf),
        "-preset",
        str(preset),
        "-svtav1-params",
        f"lp={lp}",
        "-pix_fmt",
        "yuv420p",
        "-r",
        str(fps),
        str(output_video),
    ]
    subprocess.run(cmd, check=True)

    output_bytes = output_video.stat().st_size
    ratio = source_bytes / output_bytes if output_bytes else 0.0
    return {
        "codec": "libsvtav1",
        "crf": crf,
        "preset": preset,
        "scale": scale,
        "fps": fps,
        "source_bytes": source_bytes,
        "output_bytes": output_bytes,
        "compression_ratio": round(ratio, 2),
    }


def build_corpus(
    input_dir: Path,
    output_path: Path,
    dataset_name: str,
    *,
    embedder: embed_image.ImageEmbedder,
    is_pdf: bool = False,
    compress: bool = True,
    video_dir: Path | None = None,
    labels: dict[str, str] | None = None,
    sample: int | None = None,
    scratch_db: str | None = None,
    preset: str = "compressed",
) -> dict:
    """Run the full image -> .nest pipeline and return a result manifest."""
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    if is_pdf:
        tmp = tempfile.TemporaryDirectory()
        tmp_dir = Path(tmp.name)
        rendered = render_pdfs(input_dir, tmp_dir)
        image_paths = [r[0] for r in rendered]
        metadata = [{"pdf_path": r[1], "page": r[2], "original_path": r[1]} for r in rendered]
    else:
        image_paths = embed_image.list_images(input_dir)
        metadata = [{"original_path": str(p)} for p in image_paths]

    if sample is not None and sample < len(image_paths):
        rng = np.random.default_rng(42)
        idx = rng.choice(len(image_paths), size=sample, replace=False)
        idx.sort()
        image_paths = [image_paths[i] for i in idx]
        metadata = [metadata[i] for i in idx]

    if not image_paths:
        raise RuntimeError(f"no images found in {input_dir}")

    video_info = None
    video_path: Path | None = None
    if compress:
        if video_dir is None:
            video_dir = output_path.parent / f"{dataset_name}-videos"
        video_dir.mkdir(parents=True, exist_ok=True)
        video_path = video_dir / f"{dataset_name}-av1.mp4"
        video_info = compress_av1(image_paths, video_path)
        frame_uris = [f"video://{video_path}#frame={i}" for i in range(len(image_paths))]
    else:
        frame_uris = [f"file://{p}" for p in image_paths]

    if compress and video_path is not None:
        # Embeddings from the compressed AV1 frames: this is what the search index
        # represents, so it is the honest recall target.
        frame_arrays = [
            embed_image.extract_frame_from_video(video_path, i) for i in range(len(image_paths))
        ]
        embeddings = embedder.embed_frames(frame_arrays)
    else:
        # Embeddings from the original images (the upper bound for recall).
        embeddings = embedder.embed(image_paths)

    cfg = BuildConfig(
        output_path=str(output_path),
        embedding_model=embedder.model_id,
        embedding_dim=embedder.dim,
        chunker_version="image-v1",
        model_hash=embedder.model_hash,
        title=f"{dataset_name} image corpus",
        version="0.1.0",
        description=f"image embeddings for {dataset_name}",
        preset=preset,
    )

    def embedder_fn(specs):
        # The pipeline calls this with missed ChunkSpecs. For images, we already
        # computed all embeddings in batch, so we feed them through the cache.
        # The cache key is chunk_id; build the chunk ids here and return.
        return [embeddings[i].tolist() for i in range(len(specs))]

    pipe = Pipeline(cfg, embedder=embedder_fn, scratch_db=scratch_db)

    for i, (img_path, uri) in enumerate(zip(image_paths, frame_uris, strict=False)):
        label = labels.get(str(img_path), "") if labels else ""
        meta = metadata[i]
        canonical = label or f"{dataset_name} {meta.get('original_path', img_path.name)}"
        # byte_start/byte_end encode the frame ordinal so the URI is resolvable.
        from builder import ChunkSpec

        pipe.add(
            ChunkSpec(
                canonical_text=canonical,
                source_uri=uri,
                byte_start=i,
                byte_end=i + 1,
            )
        )

    provenance = {
        "dataset": dataset_name,
        "n_images": len(image_paths),
        "compressed": compress,
        "video_info": video_info,
        "input_dir": str(input_dir),
    }
    pipe.emit(provenance=provenance)
    pipe.close()

    return {
        "dataset": dataset_name,
        "output": str(output_path),
        "n_images": len(image_paths),
        "video_info": video_info,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--pdf", action="store_true")
    parser.add_argument("--no-compress", action="store_true")
    parser.add_argument("--model", default="hf-hub:redlessone/DermLIP_ViT-B-16")
    parser.add_argument("--pretrained")
    parser.add_argument("--device")
    parser.add_argument("--sample", type=int)
    parser.add_argument("--preset", default="compressed")
    parser.add_argument("--scratch-db")
    parser.add_argument("--labels", type=Path, help="JSON file mapping image path -> label")
    parser.add_argument("--video-dir", type=Path)
    args = parser.parse_args()

    labels = None
    if args.labels:
        labels = json.loads(args.labels.read_text())

    embedder = embed_image.ImageEmbedder(
        model_id=args.model, pretrained=args.pretrained, device=args.device
    )
    result = build_corpus(
        input_dir=args.input_dir,
        output_path=args.output,
        dataset_name=args.dataset,
        embedder=embedder,
        is_pdf=args.pdf,
        compress=not args.no_compress,
        video_dir=args.video_dir,
        labels=labels,
        sample=args.sample,
        scratch_db=args.scratch_db,
        preset=args.preset,
    )
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
