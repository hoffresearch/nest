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
from .image_decode import decode_avif, decode_frames, decode_jxl
from .image_encode import (
    INTER_KEYINT,
    encode_av1,
    encode_avif,
    encode_jxl_dir,
    probe_gop,
    provenance_sha256,
)


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


def _resolve_keyint(paths, canvas, crf, speed, pix_fmt, gop_policy, all_intra, tune, contiguous):
    """Turn the policy into a keyint, plus the record the manifest keeps.

    `auto` runs the probe encode and lets the bytes decide (fase 0, CP-0.5:
    embedding cosine does not separate the regimes, so the policy is a
    measured encode decision, with intra as the tie-break for O(1) access).
    The legacy `all_intra` flag forces intra, as does `gop_policy="intra"`.
    """
    if all_intra or gop_policy == "intra":
        return 1, {"policy": "intra" if not all_intra else "flag", "decision": "intra"}
    # inter uses a BOUNDED gop (keyint=16), not the encoder default: measured
    # 2026-08-31 on 2787 same-artwork reprints, g=16 beat both single-keyframe
    # (85.0 vs 95.0 MB) and g=8/g=32, is -29% vs intra, and caps random-access
    # decode at 16 frames. encode_av1 pairs it with scd=0 (cards are not
    # scene cuts; scene detection re-inserts the keyframes inter exists to
    # avoid).
    if gop_policy == "inter":
        return INTER_KEYINT, {"policy": "inter", "decision": "inter", "keyint": INTER_KEYINT}
    probe = probe_gop(
        paths, canvas, crf=crf, preset=speed, pix_fmt=pix_fmt, tune=tune, contiguous=contiguous
    )
    return (1 if probe["decision"] == "intra" else INTER_KEYINT), probe


def _av1_sharded(
    paths,
    output_path,
    dataset_name,
    canvas,
    crf,
    speed,
    keyint,
    pix_fmt,
    shard_size,
    tune="default",
    fps=1,
    contiguous=False,
) -> dict:
    """Consecutive ~`shard_size`-frame segments with an index in the manifest.

    Sharding bounds the worst-case seek walk and is the shape a future
    append will merge into; true append (index merge) is NOT implemented
    and is declared as such. Each segment goes through `encode_av1`, so the
    frame-count and pix_fmt guards hold per segment.

    `keyint=None` means gop=auto resolved PER SEGMENT: one probe per shard.
    A single global probe averages regimes away — with order=cluster the
    near-duplicate runs concentrate in a few segments, and those are exactly
    where inter pays (measured 2026-08-31: -29% on same-artwork reprints)
    while unique-image segments keep O(1) all-intra access.
    """
    media_dir = image_media.media_dir_for(output_path)
    segments = []
    probes: list[dict] = []
    first: dict | None = None
    for seg_idx, start in enumerate(range(0, len(paths), shard_size)):
        chunk = paths[start : start + shard_size]
        seg_keyint = keyint
        if keyint is None:
            probe = probe_gop(
                chunk,
                canvas,
                crf=crf,
                preset=speed,
                pix_fmt=pix_fmt,
                tune=tune,
                contiguous=contiguous,
            )
            seg_keyint = 1 if probe["decision"] == "intra" else INTER_KEYINT
            probes.append({"segment": seg_idx, **probe})
        name = f"{dataset_name}-av1-{seg_idx:03d}.mp4"
        info = encode_av1(
            chunk,
            media_dir / name,
            canvas=canvas,
            crf=crf,
            preset=speed,
            keyint=seg_keyint,
            pix_fmt=pix_fmt,
            tune=tune,
            fps=fps,
        )
        first = first or info
        segments.append(
            {
                "uri": name,
                "start_frame": start,
                "n_frames": info["frame_count"],
                "output_bytes": info["output_bytes"],
                "media_sha256": info["media_sha256"],
                "keyint": seg_keyint,
            }
        )
    total = sum(s["output_bytes"] for s in segments)
    source_bytes = sum(p.stat().st_size for p in paths)
    seg_keyints = {s["keyint"] for s in segments}
    top_keyint = seg_keyints.pop() if len(seg_keyints) == 1 else None
    toolchain = dict(first["toolchain"])
    toolchain["params"] = {**toolchain["params"], "keyint": top_keyint}
    return {
        "backend": "av1",
        "codec": "libsvtav1",
        "crf": crf,
        "preset": speed,
        "keyint": top_keyint,
        "pix_fmt": first["pix_fmt"],
        "canvas": [canvas[0], canvas[1]],
        "frame_count": sum(s["n_frames"] for s in segments),
        "source_bytes": source_bytes,
        "output_bytes": total,
        "compression_ratio": round(source_bytes / total, 2) if total else 0.0,
        "shard_size": shard_size,
        "segments": segments,
        "gop_probes": probes,
        "toolchain": toolchain,
        "provenance_sha256": provenance_sha256(toolchain),
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
    tune="default",
    fps=1,
) -> dict:
    n = len(render_paths)
    order = list(order) if order is not None else list(range(n))
    paths = [render_paths[i] for i in order]
    sharded = bool(shard_size) and n > shard_size
    # an engineered order (cluster/similarity) puts the redundancy between
    # NEIGHBOURS: the probe must sample contiguous windows there, or it
    # erases the very signal the ordering created.
    ordered = any(a != b for a, b in zip(order, range(n), strict=True))
    if sharded and gop_policy == "auto" and not all_intra:
        # per-segment resolution: keyint=None tells _av1_sharded to probe
        # each shard on its own (RFC-2 pendencia 2).
        keyint, gop_record = None, None
    else:
        keyint, gop_record = _resolve_keyint(
            paths, canvas, crf, speed, pix_fmt, gop_policy, all_intra, tune, ordered
        )
    media_dir = image_media.media_dir_for(output_path)
    if sharded:
        media = _av1_sharded(
            paths,
            output_path,
            dataset_name,
            canvas,
            crf,
            speed,
            keyint,
            pix_fmt,
            shard_size,
            tune=tune,
            fps=fps,
            contiguous=ordered,
        )
        probes = media.pop("gop_probes")
        if gop_record is None:
            kinds = {p["decision"] for p in probes}
            gop_record = {
                "policy": "auto",
                "per_segment": True,
                "decision": kinds.pop() if len(kinds) == 1 else "mixed",
                "segments": probes,
            }
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
            tune=tune,
            fps=fps,
        )
        media["segments"] = [
            {
                "uri": media_name,
                "start_frame": 0,
                "n_frames": media["frame_count"],
                "output_bytes": media["output_bytes"],
                "media_sha256": media["media_sha256"],
            }
        ]
        seg_names = [media_name]
    media["gop"] = gop_record
    if ordered:
        media["order"] = "similarity-greedy"
        media["order_permutation"] = list(order)

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
    tune: str = "default",
    fps: int = 1,
    jxl_transcode=None,
) -> dict:
    """Build the media side of a corpus and return its manifest record."""
    assert canvas is not None
    if control:
        return _control(render_paths, output_path, dataset_name, canvas)
    if backend == "avif":
        return _avif(render_paths, output_path, dataset_name, canvas, pix_fmt, avif_quality)
    if backend in ("jxl", "jxl-transcode"):
        return _jxl(
            render_paths, output_path, dataset_name, backend == "jxl-transcode", jxl_transcode
        )
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
        tune=tune,
        fps=fps,
    )


def _jxl(render_paths, output_path, dataset_name, transcode: bool, policy) -> dict:
    """One .jxl per SOURCE image, no letterbox: the whole point is losslessness.

    `jxl` mode is lossless of the source pixels; `jxl-transcode` is the
    bit-exact reversible JPEG repack. Decoded frames vary in size, which the
    embed pass handles; the uniform-canvas contract belongs to the lossy
    stream backends.
    """
    jxl_dir = image_media.media_dir_for(output_path) / f"{dataset_name}-jxl"
    media = encode_jxl_dir(
        render_paths,
        jxl_dir,
        transcode=transcode,
        on_unsupported_jpeg=getattr(policy, "on_unsupported_jpeg", "copy-source"),
        verify_roundtrip=getattr(policy, "verify_roundtrip", True),
    )
    uris = [f"media://{dataset_name}-jxl/{name}" for name in media["files"]]

    def jxl_frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
        batch: list[np.ndarray] = []
        for name in media["files"]:
            batch.append(decode_jxl(jxl_dir / name))
            if len(batch) == batch_size:
                yield batch
                batch = []
        if batch:
            yield batch

    return {"media": media, "uris": uris, "frames": jxl_frames}


def decoded_frames_fn(media_dir: Path, media: dict, frame_uris: Sequence[str]):
    """Rebuild the decoded-frames iterator for an EXISTING media set (resume
    path): what `build_media` hands back at encode time, reconstructed from
    the manifest record. av1 yields STREAM order (the caller un-permutes
    with `order_permutation`); per-image backends yield item order."""
    backend = media.get("backend", "")
    if backend == "av1":
        canvas = tuple(media["canvas"])
        seg_names = [s["uri"] for s in media["segments"]]

        def frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
            for name in seg_names:
                yield from decode_frames(media_dir / name, canvas, batch_size=batch_size)

        return frames

    rel_paths = [u.removeprefix("media://") for u in frame_uris]

    def _load(rel: str) -> np.ndarray:
        from PIL import Image

        path = media_dir / rel
        if path.suffix == ".jxl":
            return decode_jxl(path)
        if path.suffix == ".avif":
            return decode_avif(path)
        with Image.open(path) as img:
            return np.asarray(img.convert("RGB"), dtype=np.uint8)

    def per_image_frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
        # per-image decode is one subprocess per file: fan it out on a
        # bounded window (order preserved, ~window frames in flight) —
        # sequential djxl over a 38k corpus is hours, this is minutes.
        # decoding is deterministic, so bytes are unchanged.
        import os
        from collections import deque
        from concurrent.futures import ThreadPoolExecutor
        from itertools import islice

        window = 32
        with ThreadPoolExecutor(max_workers=min(8, os.cpu_count() or 1)) as pool:
            it = iter(rel_paths)
            futs = deque(pool.submit(_load, rel) for rel in islice(it, window))
            batch: list[np.ndarray] = []
            while futs:
                arr = futs.popleft().result()
                nxt = next(it, None)
                if nxt is not None:
                    futs.append(pool.submit(_load, nxt))
                batch.append(arr)
                if len(batch) == batch_size:
                    yield batch
                    batch = []
            if batch:
                yield batch

    return per_image_frames
