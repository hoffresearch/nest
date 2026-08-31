"""Encode side of image corpus media: av1 stream and avif per-image.

Backend selection and the control corpus live in `forge/image_backends.py`;
this module keeps the two encoders and their shared provenance record.

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
import tempfile
from collections.abc import Sequence
from pathlib import Path

import numpy as np

from . import image_media


def _tool_version(cmd: list[str]) -> str:
    out = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return out.stdout.splitlines()[0].strip()


def provenance_sha256(payload: dict) -> str:
    """Fingerprint the toolchain record, canonically."""
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(canonical.encode()).hexdigest()


def probe_tune_still(cache: dict = {}) -> int | None:  # noqa: B006
    """Resolve the SVT-AV1 'Still Picture' tune number for the local encoder.

    The number varies by SVT-AV1 version, so it is probed (a 16x16
    one-frame encode) instead of assumed: an unsupported value must become
    a loud warning and a recorded fallback, never a silently ignored flag.
    Cached per process.
    """
    if "value" in cache:
        return cache["value"]
    import sys

    for candidate in (3, 4):
        with tempfile.NamedTemporaryFile(suffix=".mp4") as tmp:
            # fmt: off
            cmd = [
                "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "color=black:size=16x16:rate=1",
                "-frames:v", "1", "-c:v", "libsvtav1",
                "-svtav1-params", f"tune={candidate}", tmp.name,
            ]
            # fmt: on
            if subprocess.run(cmd, capture_output=True).returncode == 0:
                cache["value"] = candidate
                return candidate
    print(
        "[forge] warning: local SVT-AV1 has no Still Picture tune; using default tune",
        file=sys.stderr,
    )
    cache["value"] = None
    return None


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
    tune: str = "default",
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
    tune_resolved: int | None = None
    if tune == "still":
        tune_resolved = probe_tune_still()
        if tune_resolved is not None:
            svt_params += f":tune={tune_resolved}"

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
            "crf": crf,
            "preset": preset,
            "fps": fps,
            "lp": lp,
            "keyint": keyint,
            "pix_fmt": actual_fmt,
            "tune": tune,
            "tune_resolved": tune_resolved,
        },
    }
    output_bytes = output_path.stat().st_size
    return {
        "backend": "av1",
        "codec": "libsvtav1",
        "crf": crf,
        "preset": preset,
        "fps": fps,
        "tune": tune,
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


def probe_gop(
    image_paths: Sequence[Path],
    canvas: tuple[int, int],
    *,
    crf: int = 35,
    preset: int = 8,
    pix_fmt: str = "yuv420p",
    n_samples: int = 32,
) -> dict:
    """Encode a spaced sample both ways and let the bytes decide the gop.

    Cosine similarity of the embeddings did NOT separate intra-favouring
    from inter-favouring corpora in fase 0 (CP-0.5), so the policy is
    decided by the only thing that actually pays: a probe encode at the
    target crf. The sample is evenly spaced, so scan-ordered sources (wsi
    tiles) cannot hide their redundancy in one neighbourhood. Ties go
    intra: random access is O(1) there and costs nothing extra.

    Both probe encodes go through `encode_av1`, so the frame-count and
    pix_fmt guards apply to the probe exactly as they do to the build.
    """
    paths = list(image_paths)
    n = len(paths)
    if n < 2:
        return {
            "policy": "auto",
            "n_samples": n,
            "crf": crf,
            "decision": "intra",
            "reason": "single frame",
        }
    take = min(n_samples, n)
    idx = sorted(set(np.linspace(0, n - 1, take).round().astype(int).tolist()))
    sample = [paths[i] for i in idx]
    with tempfile.TemporaryDirectory(prefix="nest-gop-probe-") as tmp:
        intra = encode_av1(
            sample,
            Path(tmp) / "intra.mp4",
            canvas=canvas,
            crf=crf,
            preset=preset,
            keyint=1,
            pix_fmt=pix_fmt,
        )
        inter = encode_av1(
            sample, Path(tmp) / "inter.mp4", canvas=canvas, crf=crf, preset=preset, pix_fmt=pix_fmt
        )
    decision = "intra" if intra["output_bytes"] <= inter["output_bytes"] else "inter"
    return {
        "policy": "auto",
        "n_samples": len(sample),
        "crf": crf,
        "intra_bytes": intra["output_bytes"],
        "inter_bytes": inter["output_bytes"],
        "decision": decision,
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


_JPEG_SUFFIXES = {".jpg", ".jpeg"}


def encode_jxl_dir(
    image_paths: Sequence[Path],
    out_dir: Path,
    *,
    transcode: bool,
    on_unsupported_jpeg: str = "copy-source",
    verify_roundtrip: bool = True,
) -> dict:
    """One .jxl per source image: `transcode` = bit-exact reversible JPEG
    repack (--lossless_jpeg=1), else lossless of the source pixels (-d 0).

    Preservation contract: transcode preserves the original JPEG BYTES
    (verified by reconstructing with djxl and comparing sha256 when
    `verify_roundtrip`); lossless preserves decoded pixels. Timestamps and
    filenames live in the manifest only. A JPEG the encoder refuses follows
    `on_unsupported_jpeg` (error | copy-source | lossless-jxl) and the
    per-file decision is recorded — a silent fallback would claim a
    reversibility the corpus does not have.
    """
    import shutil

    for tool in ("cjxl",) + (("djxl",) if (transcode and verify_roundtrip) else ()):
        if shutil.which(tool) is None:
            raise RuntimeError(f"jxl backend needs '{tool}' on PATH: brew install jpeg-xl")
    out_dir.mkdir(parents=True, exist_ok=True)
    files: list[str] = []
    decisions: list[dict] = []
    source_bytes = 0

    def run_cjxl(src: Path, dst: Path, args: list[str]) -> bool:
        return (
            subprocess.run(["cjxl", str(src), str(dst), *args], capture_output=True).returncode == 0
        )

    for i, path in enumerate(image_paths):
        source_bytes += path.stat().st_size
        name = f"{i:06d}.jxl"
        action, verified = "lossless", None
        if transcode:
            supported = path.suffix.lower() in _JPEG_SUFFIXES and run_cjxl(
                path, out_dir / name, ["--lossless_jpeg=1"]
            )
            if supported:
                action = "transcode"
                if verify_roundtrip:
                    verified = _verify_jpeg_roundtrip(path, out_dir / name)
                    if not verified:
                        (out_dir / name).unlink(missing_ok=True)
                        supported = False
            if not supported:
                if on_unsupported_jpeg == "error":
                    raise RuntimeError(
                        f"jxl-transcode: {path} is not a reversibly-transcodable jpeg"
                    )
                if on_unsupported_jpeg == "copy-source":
                    name = f"{i:06d}{path.suffix.lower()}"
                    (out_dir / name).write_bytes(path.read_bytes())
                    action, verified = "copied", True
                else:  # lossless-jxl
                    if not run_cjxl(path, out_dir / name, ["-d", "0"]):
                        raise RuntimeError(f"cjxl -d 0 failed for {path}")
                    action, verified = "lossless", None
        else:
            if not run_cjxl(path, out_dir / name, ["-d", "0"]):
                raise RuntimeError(f"cjxl -d 0 failed for {path}")
        files.append(name)
        decisions.append({"file": name, "action": action, "verified": verified})

    output_bytes = sum((out_dir / f).stat().st_size for f in files)
    toolchain = {"cjxl": _tool_version(["cjxl", "--version"]), "transcode": transcode}
    return {
        "backend": "jxl-transcode" if transcode else "jxl",
        "canvas": None,
        "frame_count": len(files),
        "files": files,
        "decisions": decisions,
        "source_bytes": source_bytes,
        "output_bytes": output_bytes,
        "compression_ratio": round(source_bytes / output_bytes, 2) if output_bytes else 0.0,
        "toolchain": toolchain,
        "provenance_sha256": provenance_sha256(toolchain),
    }


def _verify_jpeg_roundtrip(original: Path, jxl_path: Path) -> bool:
    """Reconstruct the JPEG from the .jxl and compare bytes exactly."""
    with tempfile.NamedTemporaryFile(suffix=".jpg") as tmp:
        proc = subprocess.run(["djxl", str(jxl_path), tmp.name], capture_output=True)
        if proc.returncode != 0:
            return False
        rebuilt = hashlib.sha256(Path(tmp.name).read_bytes()).hexdigest()
    return rebuilt == hashlib.sha256(original.read_bytes()).hexdigest()
