"""Backend orchestration for image corpus builds.

One contract over three backends: the builder gets `media` (the manifest
block), `uris` (one per item), and `frames()`, an iterator of decoded RGB
batches for the embed pass. The index always describes the DECODED pixels,
never the sources, or it reports a quality the corpus does not have.
"""

from __future__ import annotations

import tempfile
from collections.abc import Iterator, Sequence
from pathlib import Path

import numpy as np

from . import image_media
from .image_decode import decode_avif, decode_frames
from .image_encode import encode_av1, encode_avif


def _png_frames(png_dir: Path, batch_size: int = 32) -> Iterator[list[np.ndarray]]:
    from PIL import Image

    batch: list[np.ndarray] = []
    for out in sorted(png_dir.glob("*.png")):
        with Image.open(out) as img:
            batch.append(np.asarray(img.convert("RGB"), dtype=np.uint8))
        if len(batch) == batch_size:
            yield batch
            batch = []
    if batch:
        yield batch


def _letterbox_all(
    render_paths: Sequence[Path], canvas: tuple[int, int], out_dir: Path
) -> list[Path]:
    from PIL import Image

    out_dir.mkdir(parents=True, exist_ok=True)
    written = []
    for i, path in enumerate(render_paths):
        out = out_dir / f"{i:06d}.png"
        if not out.exists():
            with Image.open(path) as img:
                image_media.letterbox(img, canvas).save(out)
        written.append(out)
    return written


def _control(render_paths, output_path, dataset_name, canvas) -> dict:
    """Letterbox-lossless control corpus: the ruler the codec cost is
    measured against (fase 0, CP-0.1)."""
    png_dir = image_media.media_dir_for(output_path) / f"{dataset_name}-png"
    _letterbox_all(render_paths, canvas, png_dir)
    media = {
        "backend": "png-lossless",
        "canvas": [canvas[0], canvas[1]],
        "frame_count": len(render_paths),
    }
    uris = [f"media://{dataset_name}-png/{i:06d}.png" for i in range(len(render_paths))]
    frames = lambda batch_size=32: _png_frames(png_dir, batch_size)  # noqa: E731
    return {"media": media, "uris": uris, "frames": frames}


def _avif(render_paths, output_path, dataset_name, canvas, pix_fmt, avif_quality) -> dict:
    """One avif per image. The value is per-image O(1) semantics and real
    yuv444 (CP-0.6 asks for it on medical corpora), not compression: the
    stream won the size-matched matrix. Letterboxed onto the same canvas,
    so the corpus contract holds across backends."""
    avif_dir = image_media.media_dir_for(output_path) / f"{dataset_name}-avif"
    with tempfile.TemporaryDirectory(prefix="nest-avif-src-") as tmp:
        tmp_pngs = _letterbox_all(render_paths, canvas, Path(tmp))
        yuv = {"yuv420p": "420", "yuv444p": "444"}[pix_fmt]
        media = encode_avif(tmp_pngs, avif_dir, quality=avif_quality, yuv=yuv)
    media["canvas"] = [canvas[0], canvas[1]]
    uris = [f"media://{dataset_name}-avif/{i:06d}.avif" for i in range(len(render_paths))]

    def avif_frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
        batch: list[np.ndarray] = []
        for frame in sorted(avif_dir.glob("*.avif")):
            batch.append(decode_avif(frame))
            if len(batch) == batch_size:
                yield batch
                batch = []
        if batch:
            yield batch

    return {"media": media, "uris": uris, "frames": avif_frames}


def _av1(render_paths, output_path, dataset_name, canvas, crf, speed, all_intra, pix_fmt) -> dict:
    media_name = f"{dataset_name}-av1.mp4"
    media_path = image_media.media_dir_for(output_path) / media_name
    media = encode_av1(
        render_paths,
        media_path,
        canvas=canvas,
        crf=crf,
        preset=speed,
        keyint=1 if all_intra else None,
        pix_fmt=pix_fmt,
    )
    uris = [f"media://{media_name}#frame={i}" for i in range(len(render_paths))]
    frames = lambda batch_size=32: decode_frames(media_path, canvas, batch_size=batch_size)  # noqa: E731
    return {"media": media, "uris": uris, "frames": frames}


def build_media(
    render_paths: Sequence[Path],
    output_path: Path,
    dataset_name: str,
    *,
    backend: str,
    canvas: tuple[int, int] | None,
    crf: int,
    speed: int,
    all_intra: bool,
    pix_fmt: str,
    avif_quality: int,
    control: bool,
) -> dict:
    """Build the media side of a corpus and return its manifest record."""
    assert canvas is not None
    if control:
        return _control(render_paths, output_path, dataset_name, canvas)
    if backend == "avif":
        return _avif(render_paths, output_path, dataset_name, canvas, pix_fmt, avif_quality)
    return _av1(render_paths, output_path, dataset_name, canvas, crf, speed, all_intra, pix_fmt)
