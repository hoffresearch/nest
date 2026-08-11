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
from .image_encode import encode_av1, encode_avif, probe_gop


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
    measured against (fase 0, CP-0.1). Its byte size is recorded so every
    variant can be reported against the SAME ruler; the per-backend
    `source_bytes` are not comparable (av1 sums the original files, avif
    sums its letterboxed png inputs)."""
    png_dir = image_media.media_dir_for(output_path) / f"{dataset_name}-png"
    written = _letterbox_all(render_paths, canvas, png_dir)
    media = {
        "backend": "png-lossless",
        "canvas": [canvas[0], canvas[1]],
        "frame_count": len(render_paths),
        "output_bytes": sum(p.stat().st_size for p in written),
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


def _resolve_keyint(paths, canvas, crf, speed, pix_fmt, gop_policy, all_intra):
    """Turn the policy into a keyint, plus the record the manifest keeps.

    `auto` runs the probe encode and lets the bytes decide (fase 0, CP-0.5:
    embedding cosine does not separate the regimes, so the policy is a
    measured encode decision, with intra as the tie-break for O(1) access).
    The legacy `all_intra` flag forces intra, as does `gop_policy="intra"`.
    """
    if all_intra or gop_policy == "intra":
        return 1, {"policy": "intra" if not all_intra else "flag", "decision": "intra"}
    if gop_policy == "inter":
        return None, {"policy": "inter", "decision": "inter"}
    probe = probe_gop(paths, canvas, crf=crf, preset=speed, pix_fmt=pix_fmt)
    return (1 if probe["decision"] == "intra" else None), probe


def _av1_sharded(
    paths, output_path, dataset_name, canvas, crf, speed, keyint, pix_fmt, shard_size
) -> dict:
    """Consecutive ~`shard_size`-frame segments with an index in the manifest.

    Sharding bounds the worst-case seek walk and is the shape a future
    append will merge into; true append (index merge) is NOT implemented
    and is declared as such. Each segment goes through `encode_av1`, so the
    frame-count and pix_fmt guards hold per segment.
    """
    media_dir = image_media.media_dir_for(output_path)
    segments = []
    first: dict | None = None
    for seg_idx, start in enumerate(range(0, len(paths), shard_size)):
        chunk = paths[start : start + shard_size]
        name = f"{dataset_name}-av1-{seg_idx:03d}.mp4"
        info = encode_av1(
            chunk,
            media_dir / name,
            canvas=canvas,
            crf=crf,
            preset=speed,
            keyint=keyint,
            pix_fmt=pix_fmt,
        )
        first = first or info
        segments.append(
            {
                "uri": name,
                "start_frame": start,
                "n_frames": info["frame_count"],
                "output_bytes": info["output_bytes"],
                "media_sha256": info["media_sha256"],
            }
        )
    total = sum(s["output_bytes"] for s in segments)
    source_bytes = sum(p.stat().st_size for p in paths)
    return {
        "backend": "av1",
        "codec": "libsvtav1",
        "crf": crf,
        "preset": speed,
        "keyint": keyint,
        "pix_fmt": first["pix_fmt"],
        "canvas": [canvas[0], canvas[1]],
        "frame_count": sum(s["n_frames"] for s in segments),
        "source_bytes": source_bytes,
        "output_bytes": total,
        "compression_ratio": round(source_bytes / total, 2) if total else 0.0,
        "shard_size": shard_size,
        "segments": segments,
        "toolchain": first["toolchain"],
        "provenance_sha256": first["provenance_sha256"],
    }


def _av1(
    render_paths,
    output_path,
    dataset_name,
    canvas,
    crf,
    speed,
    all_intra,
    pix_fmt,
    gop_policy,
    order,
    shard_size,
) -> dict:
    n = len(render_paths)
    order = list(order) if order is not None else list(range(n))
    paths = [render_paths[i] for i in order]
    keyint, gop_record = _resolve_keyint(paths, canvas, crf, speed, pix_fmt, gop_policy, all_intra)
    media_dir = image_media.media_dir_for(output_path)
    if shard_size and n > shard_size:
        media = _av1_sharded(
            paths, output_path, dataset_name, canvas, crf, speed, keyint, pix_fmt, shard_size
        )
        seg_names = [s["uri"] for s in media["segments"]]
    else:
        media_name = f"{dataset_name}-av1.mp4"
        media = encode_av1(
            paths,
            media_dir / media_name,
            canvas=canvas,
            crf=crf,
            preset=speed,
            keyint=keyint,
            pix_fmt=pix_fmt,
        )
        seg_names = [media_name]
    media["gop"] = gop_record
    if any(a != b for a, b in zip(order, range(n), strict=True)):
        media["order"] = "similarity-greedy"

    # uris are returned in ITEM order: item i names the stream position the
    # permutation carried it to. vectors/hashes come back in stream order
    # and the caller un-permutes them with `order`.
    sizes = [s["n_frames"] for s in media["segments"]] if "segments" in media else [n]
    bounds = np.cumsum([0, *sizes])
    uris = [""] * n
    for stream_pos, item in enumerate(order):
        seg = int(np.searchsorted(bounds, stream_pos, side="right") - 1)
        uris[item] = f"media://{seg_names[seg]}#frame={stream_pos - int(bounds[seg])}"

    def frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
        for name in seg_names:
            yield from decode_frames(media_dir / name, canvas, batch_size=batch_size)

    return {"media": media, "uris": uris, "frames": frames, "order": order}


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
    gop_policy: str = "auto",
    order: Sequence[int] | None = None,
    shard_size: int | None = None,
) -> dict:
    """Build the media side of a corpus and return its manifest record."""
    assert canvas is not None
    if control:
        return _control(render_paths, output_path, dataset_name, canvas)
    if backend == "avif":
        return _avif(render_paths, output_path, dataset_name, canvas, pix_fmt, avif_quality)
    return _av1(
        render_paths,
        output_path,
        dataset_name,
        canvas,
        crf,
        speed,
        all_intra,
        pix_fmt,
        gop_policy,
        order,
        shard_size,
    )
