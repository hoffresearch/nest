"""Encode side of image corpus media: av1 stream, avif per-image, control.

Three backends, one contract: the builder gets back `media_info` for the
manifest, the uris the chunks point at, and a `frames()` iterator that
yields the pixels the index must describe (always the DECODED pixels, never
the sources, or the index reports a quality the corpus does not have).

Provenance is recorded on every encode: the decoded pixels depend on the
exact toolchain (ffmpeg version, encoder, parameters), and another version
produces other pixels, other embeddings, and an index that silently stops
matching the media. `provenance_sha256` fingerprints that toolchain.

Every encode probes what was actually written (frame count, pix_fmt):
an encoder that falls back silently turns a flag into a lie.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from collections.abc import Iterator, Sequence
from pathlib import Path

import numpy as np

from . import image_media
from .image_decode import decode_avif, decode_frames


def _tool_version(cmd: list[str]) -> str:
    out = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return out.stdout.splitlines()[0].strip()


def provenance_sha256(payload: dict) -> str:
    """Fingerprint the toolchain record, canonically."""
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(canonical.encode()).hexdigest()


def encode_av1(
    image_paths: Sequence[Path],
    output_path: Path,
    *,
    canvas: tuple[int, int],
    crf: int = 35,
    preset: int = 8,
    fps: int = 1,
    lp: int = 2,
    keyint: int | None = None,
    pix_fmt: str = "yuv420p",
) -> dict:
    """Encode an ordered image list to one AV1 mp4 through a rawvideo pipe.

    `keyint=1` makes every frame a keyframe. Measured across three axes in
    fase 0 (ph2 dermoscopy, wsi tiles in scan order, pdf pages), all-intra
    beat the default gop on size at the same crf on every one: unrelated
    frames give motion estimation nothing to find, and every frame being a
    keyframe makes random access O(1). Still a lever, not the default,
    because the policy decision belongs to the builder's probe.

    The requested `pix_fmt` is probed after the encode and a mismatch
    raises: encoders fall back silently, and a 444 flag that writes 420
    bytes is a published lie.

    Raises if the encoded frame count disagrees with the image count: a
    corpus whose `#frame=N` pointers are off by one is worse than no corpus.
    """
    from PIL import Image

    width, height = canvas
    source_bytes = sum(p.stat().st_size for p in image_paths)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    svt_params = f"lp={lp}" + (f":keyint={keyint}" if keyint else "")

    # fmt: off
    cmd = [
        "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
        "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{width}x{height}", "-r", str(fps), "-i", "-",
        "-c:v", "libsvtav1", "-crf", str(crf), "-preset", str(preset),
        "-svtav1-params", svt_params,
        "-pix_fmt", pix_fmt, "-frames:v", str(len(image_paths)),
        str(output_path),
    ]
    # fmt: on
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        for path in image_paths:
            with Image.open(path) as img:
                frame = image_media.letterbox(img, canvas)
            proc.stdin.write(np.asarray(frame, dtype=np.uint8).tobytes())
        proc.stdin.close()
        failure = "ffmpeg encode failed" if proc.wait() != 0 else None
    except BrokenPipeError:
        proc.wait()
        failure = "ffmpeg closed the stream early"
    stderr = proc.stderr.read().decode()
    proc.stderr.close()
    if failure:
        raise RuntimeError(f"{failure}: {stderr}")

    actual_fmt = image_media.probe_pix_fmt(output_path)
    if actual_fmt != pix_fmt:
        output_path.unlink(missing_ok=True)
        raise RuntimeError(
            f"encoder wrote {actual_fmt} for a requested {pix_fmt}: it fell back "
            "silently. use the avif backend for 444, or drop the flag"
        )

    frames = image_media.probe_frame_count(output_path)
    if frames != len(image_paths):
        raise RuntimeError(
            f"encoded {frames} frames for {len(image_paths)} images; "
            "frame pointers would be misaligned"
        )

    toolchain = {
        "ffmpeg": _tool_version(["ffmpeg", "-version"]),
        "encoder": "libsvtav1",
        "params": {
            "crf": crf, "preset": preset, "fps": fps, "lp": lp,
            "keyint": keyint, "pix_fmt": actual_fmt,
        },
    }
    output_bytes = output_path.stat().st_size
    return {
        "backend": "av1",
        "codec": "libsvtav1",
        "crf": crf,
        "preset": preset,
        "fps": fps,
        "keyint": keyint,
        "pix_fmt": actual_fmt,
        "canvas": [width, height],
        "frame_count": frames,
        "source_bytes": source_bytes,
        "output_bytes": output_bytes,
        "compression_ratio": round(source_bytes / output_bytes, 2) if output_bytes else 0.0,
        "media_sha256": image_media.sha256_file(output_path),
        "toolchain": toolchain,
        "provenance_sha256": provenance_sha256(toolchain),
    }


def encode_avif(
    image_paths: Sequence[Path],
    out_dir: Path,
    *,
    quality: int = 35,
    yuv: str = "420",
    speed: int = 8,
) -> dict:
    """Encode one avif per image into `out_dir`, with the same uri contract.

    The avif backend's value is not compression (measured in fase 0: the
    av1 stream wins at every matched size on ph2); it is per-image O(1)
    semantics without an ordinal or an alignment guard, and real yuv444,
    which the measured melanoma breakdown (CP-0.6) asks for on medical
    corpora. The requested `yuv` is verified with `avifdec --info` on the
    first file.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    source_bytes = 0
    total = 0
    for path in image_paths:
        source_bytes += path.stat().st_size
        out = out_dir / f"{Path(path).stem}.avif"
        # fmt: off
        cmd = [
            "avifenc", "-q", str(quality), "--speed", str(speed),
            "--yuv", yuv, str(path), str(out),
        ]
        # fmt: on
        proc = subprocess.run(cmd, capture_output=True)
        if proc.returncode != 0:
            raise RuntimeError(f"avifenc failed on {path}: {proc.stderr.decode()[-400:]}")
        total += out.stat().st_size

    probe = subprocess.run(
        ["avifdec", "--info", str(out_dir / f"{Path(image_paths[0]).stem}.avif")],
        capture_output=True,
        text=True,
    )
    actual_yuv = "444" if "YUV444" in probe.stdout else "420"
    if actual_yuv != yuv:
        raise RuntimeError(f"avifenc wrote yuv{actual_yuv} for a requested yuv{yuv}")

    toolchain = {
        "ffmpeg": _tool_version(["avifenc", "--version"]),
        "encoder": "avifenc/aom",
        "params": {"quality": quality, "speed": speed, "yuv": actual_yuv},
    }
    return {
        "backend": "avif",
        "codec": "avifenc",
        "quality": quality,
        "speed": speed,
        "yuv": actual_yuv,
        "frame_count": len(image_paths),
        "source_bytes": source_bytes,
        "output_bytes": total,
        "compression_ratio": round(source_bytes / total, 2) if total else 0.0,
        "toolchain": toolchain,
        "provenance_sha256": provenance_sha256(toolchain),
    }


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
    """Build the media side of a corpus and return its manifest record.

    Returns `media` (the manifest block), `uris` (one per item), and
    `frames()`, an iterator of decoded RGB batches for the embed pass.
    `control=True` ignores `backend` and writes letterboxed lossless pngs:
    the control corpus the codec cost is measured against.
    """
    media_dir = image_media.media_dir_for(output_path)
    if control:
        from PIL import Image

        assert canvas is not None
        png_dir = media_dir / f"{dataset_name}-png"
        png_dir.mkdir(parents=True, exist_ok=True)
        for i, path in enumerate(render_paths):
            out = png_dir / f"{i:06d}.png"
            if not out.exists():
                with Image.open(path) as img:
                    image_media.letterbox(img, canvas).save(out)
        media = {
            "backend": "png-lossless",
            "canvas": [canvas[0], canvas[1]],
            "frame_count": len(render_paths),
        }
        uris = [f"media://{dataset_name}-png/{i:06d}.png" for i in range(len(render_paths))]

        def control_frames(batch_size: int = 32) -> Iterator[list[np.ndarray]]:
            batch: list[np.ndarray] = []
            for out in sorted(png_dir.glob("*.png")):
                with Image.open(out) as img:
                    batch.append(np.asarray(img.convert("RGB"), dtype=np.uint8))
                if len(batch) == batch_size:
                    yield batch
                    batch = []
            if batch:
                yield batch

        return {"media": media, "uris": uris, "frames": control_frames}

    if backend == "avif":
        from PIL import Image

        assert canvas is not None
        # the avif path letterboxes onto the same canvas as the stream: the
        # corpus contract (uniform geometry, symmetric queries) holds across
        # backends, and avifenc only accepts file input anyway.
        import tempfile

        avif_dir = media_dir / f"{dataset_name}-avif"
        with tempfile.TemporaryDirectory(prefix="nest-avif-src-") as tmp:
            tmp_pngs = []
            for i, path in enumerate(render_paths):
                out = Path(tmp) / f"{i:06d}.png"
                with Image.open(path) as img:
                    image_media.letterbox(img, canvas).save(out)
                tmp_pngs.append(out)
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

    assert canvas is not None
    media_name = f"{dataset_name}-av1.mp4"
    media_path = media_dir / media_name
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
