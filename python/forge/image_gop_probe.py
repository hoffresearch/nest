"""The gop probe behind `gop_policy=auto`: encode a sample both ways, decide.

Carved out of `forge/image_encode.py`; both arms go through `encode_av1`
there, so the frame-count and pix_fmt guards apply to the probe exactly as
they do to the build.
"""

from __future__ import annotations

import tempfile
from collections.abc import Sequence
from pathlib import Path

import numpy as np

from . import image_media
from .image_encode import INTER_KEYINT, encode_av1

# inter must not buy bytes with quality: at the SAME crf SVT quantizes
# P-frames far coarser, and a bytes-only probe is blind to it (measured
# 2026-08-31 on unique cards: -10.7% bytes for -15.9 ssimulacra2 p50).
# inter wins only when its mean ssimulacra2 on the probe sample is within
# this tolerance of the intra arm.
PROBE_QUALITY_TOL = 2.0


def probe_gop(
    image_paths: Sequence[Path],
    canvas: tuple[int, int],
    *,
    crf: int = 35,
    preset: int = 8,
    pix_fmt: str = "yuv420p",
    n_samples: int = 32,
    tune: str = "default",
    contiguous: bool = False,
) -> dict:
    """Encode a sample both ways and let the bytes decide the gop.

    Cosine similarity of the embeddings did NOT separate intra-favouring
    from inter-favouring corpora in fase 0 (CP-0.5), so the policy is
    decided by the only thing that actually pays: a probe encode at the
    target crf. Ties go intra: random access is O(1) there and costs
    nothing extra.

    Sampling has two modes. Default is evenly spaced, so scan-ordered
    sources (wsi tiles) cannot hide their redundancy in one neighbourhood.
    `contiguous=True` is for ENGINEERED adjacency (order=cluster/
    similarity): there the redundancy lives between neighbours, and evenly
    spaced frames would erase the very signal the ordering created — so
    the probe takes contiguous windows spread across the segment instead.

    Each arm probes what would actually ship: the intra arm carries the
    requested tune, the inter arm drops a still tune the same way the real
    encode does (SVT's IQ tune is all-intra only). Both go through
    `encode_av1`, so the frame-count and pix_fmt guards apply to the probe
    exactly as they do to the build.
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
    if contiguous:
        win = min(8, take)
        starts = sorted({int(s) for s in np.linspace(0, n - win, max(1, take // win))})
        idx = sorted({i for s in starts for i in range(s, s + win)})
    else:
        idx = sorted(set(np.linspace(0, n - 1, take).round().astype(int).tolist()))
    sample = [paths[i] for i in idx]
    quality: dict = {}
    with tempfile.TemporaryDirectory(prefix="nest-gop-probe-") as tmp:
        intra = encode_av1(
            sample,
            Path(tmp) / "intra.mp4",
            canvas=canvas,
            crf=crf,
            preset=preset,
            keyint=1,
            pix_fmt=pix_fmt,
            tune=tune,
        )
        inter = encode_av1(
            sample,
            Path(tmp) / "inter.mp4",
            canvas=canvas,
            crf=crf,
            preset=preset,
            keyint=INTER_KEYINT,  # probe what would actually ship
            pix_fmt=pix_fmt,
            tune=tune,  # dropped for inter inside encode_av1, like the build
        )
        quality = _probe_arm_quality(sample, canvas, Path(tmp))
    decision = "intra" if intra["output_bytes"] <= inter["output_bytes"] else "inter"
    # inter may not pay its byte savings with quality (a bytes-only decision
    # at fixed crf is blind to SVT's coarser P-frame quantization).
    if (
        decision == "inter"
        and quality.get("intra_ssim2") is not None
        and quality["intra_ssim2"] - quality["inter_ssim2"] > PROBE_QUALITY_TOL
    ):
        decision = "intra"
        quality["overridden"] = "inter-degrades-quality"
    return {
        "policy": "auto",
        "n_samples": len(sample),
        "sampling": "contiguous-windows" if contiguous else "evenly-spaced",
        "crf": crf,
        "tune": tune,
        "intra_bytes": intra["output_bytes"],
        "inter_bytes": inter["output_bytes"],
        **quality,
        "decision": decision,
    }


def _probe_arm_quality(sample: Sequence[Path], canvas, tmp: Path) -> dict:
    """Mean ssimulacra2 of both probe arms against the letterboxed sources.

    Soft dependency: without `ssimulacra2` on PATH the probe stays
    bytes-only and says so in the record (and once on stderr) instead of
    failing builds that never asked for a quality gate.
    """
    import shutil as _shutil
    import sys

    if _shutil.which("ssimulacra2") is None:
        print(
            "[forge] warning: ssimulacra2 not on PATH; gop probe is bytes-only "
            "(brew install jpeg-xl)",
            file=sys.stderr,
        )
        return {"quality": "unmeasured (ssimulacra2 not on PATH)"}
    from PIL import Image

    from .image_decode import decode_frames
    from .quality_gate import _ssimulacra2

    src_pngs = []
    for i, p in enumerate(sample):
        out = tmp / f"src{i:04d}.png"
        with Image.open(p) as img:
            image_media.letterbox(img, canvas).save(out)
        src_pngs.append(out)
    scores = {}
    for arm in ("intra", "inter"):
        vals = []
        i = 0
        for batch in decode_frames(tmp / f"{arm}.mp4", canvas, batch_size=16):
            for frame in batch:
                dist = tmp / f"{arm}{i:04d}.png"
                Image.fromarray(frame).save(dist)
                vals.append(_ssimulacra2(src_pngs[i], dist))
                i += 1
        scores[f"{arm}_ssim2"] = round(float(np.mean(vals)), 2)
    scores["quality_tolerance"] = PROBE_QUALITY_TOL
    return scores
