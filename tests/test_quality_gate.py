"""Prove the dual quality gate (RFC-2): stratified SSIMULACRA2 floors and
the embedding-drift floor are BOTH enforced (either alone can reject a crf);
an unattainable floor falls back to the smallest ladder crf with a warning;
jxl-transcode round-trips JPEG bytes exactly and follows the fallback policy
for non-JPEG sources.

Skips cleanly without ffmpeg / ssimulacra2 / cjxl.

Run: .venv/bin/python tests/test_quality_gate.py
"""

import shutil
import sys
import tempfile
from dataclasses import replace
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))

import numpy as np
from forge.build_spec import MediaSpec, QualitySpec
from forge.image_encode import encode_jxl_dir

HAVE = all(shutil.which(t) for t in ("ffmpeg", "ssimulacra2", "cjxl", "djxl"))


class LocalityAdapter:
    """Deterministic no-ML gate model WITH locality: 4x4 mean-pool of RGB,
    L2-normalized. Small pixel changes move the vector a little, not
    randomly — what the drift leg needs to be testable."""

    batch_size = 8
    model_hash = "sha256:" + "ab" * 32

    def embed_arrays(self, frames):
        out = []
        for f in frames:
            f = np.asarray(f, dtype=np.float32)
            h, w = f.shape[0] // 4, f.shape[1] // 4
            pooled = f[: h * 4, : w * 4].reshape(4, h, 4, w, 3).mean(axis=(1, 3)).ravel()
            out.append(pooled / (np.linalg.norm(pooled) or 1.0))
        return np.stack(out)


def _images(base: Path) -> list[Path]:
    from PIL import Image

    rng = np.random.default_rng(3)
    paths = []
    for i in range(8):
        arr = (
            rng.integers(0, 255, (96, 96, 3), dtype=np.uint8)  # high-frequency noise
            if i < 4
            else np.full((96, 96, 3), 40 + i * 9, dtype=np.uint8)  # flat
        )
        p = base / f"q{i}.png"
        Image.fromarray(arr).save(p)
        paths.append(p)
    return paths


def _media(**q) -> MediaSpec:
    m = MediaSpec(crf="auto", speed=13)
    m.quality = replace(
        QualitySpec(buckets=["entropy"], sample_per_bucket=4, crf_ladder=[35, 50]), **q
    )
    return m


def test_gate_structure_and_choice(base: Path) -> None:
    from forge.quality_gate import choose_crf

    paths = _images(base)
    m = _media(visual_floor_p10=-1e9, visual_floor_min=-1e9, drift_floor_p10=-2)
    crf, report = choose_crf(paths, (96, 96), m, LocalityAdapter())
    assert crf == 50, "with no floors the largest ladder crf must win"
    assert report["bucket_heuristics_version"] == 1
    ladder = report["ladder"]["35"]
    assert "ssim_p10_by_bucket" in ladder and "drift_p10" in ladder
    assert len(ladder["ssim_p10_by_bucket"]) >= 2, "noise and flat must land in strata"
    print("test_gate_structure_and_choice: OK")


def test_visual_floor_rejects(base: Path) -> None:
    from forge.quality_gate import choose_crf

    paths = _images(base)
    m = _media(visual_floor_p10=101, visual_floor_min=-1e9, drift_floor_p10=-2)
    crf, report = choose_crf(paths, (96, 96), m, LocalityAdapter())
    assert crf == 35 and report["warning"], "unattainable visual floor -> smallest crf + warning"
    print("test_visual_floor_rejects: OK")


def test_drift_floor_rejects_alone(base: Path) -> None:
    from forge.quality_gate import choose_crf

    paths = _images(base)
    m = _media(visual_floor_p10=-1e9, visual_floor_min=-1e9, drift_floor_p10=1.01)
    crf, report = choose_crf(paths, (96, 96), m, LocalityAdapter())
    assert crf == 35 and report["warning"], "drift floor alone must be able to reject"
    assert all(not r["pass"] for r in report["ladder"].values())
    print("test_drift_floor_rejects_alone: OK")


def test_jxl_transcode_roundtrip(base: Path) -> None:
    from PIL import Image

    rng = np.random.default_rng(5)
    jpg = base / "photo.jpg"
    Image.fromarray(rng.integers(0, 255, (64, 64, 3), dtype=np.uint8)).save(jpg, quality=90)
    png = base / "notjpeg.png"
    Image.fromarray(rng.integers(0, 255, (32, 32, 3), dtype=np.uint8)).save(png)

    out = base / "jxl-out"
    media = encode_jxl_dir([jpg, png], out, transcode=True, on_unsupported_jpeg="copy-source")
    d = {x["file"]: x for x in media["decisions"]}
    assert d["000000.jxl"]["action"] == "transcode" and d["000000.jxl"]["verified"] is True
    assert d["000001.png"]["action"] == "copied"
    assert (out / "000001.png").read_bytes() == png.read_bytes()
    try:
        encode_jxl_dir([png], base / "jxl-err", transcode=True, on_unsupported_jpeg="error")
    except RuntimeError as e:
        assert "transcodable" in str(e)
    else:
        raise AssertionError("policy=error must raise on non-jpeg")
    lossless = encode_jxl_dir([png], base / "jxl-ll", transcode=False)
    assert lossless["backend"] == "jxl" and lossless["frame_count"] == 1
    print("test_jxl_transcode_roundtrip: OK")


def test_cluster_order_deterministic() -> None:
    from forge.image_order import cluster_order

    rng = np.random.default_rng(9)
    a, b = rng.standard_normal(8), rng.standard_normal(8)
    vecs = np.stack(
        [  # two tight clusters, interleaved by ordinal
            a + rng.standard_normal(8) * 0.01 if i % 2 == 0 else b + rng.standard_normal(8) * 0.01
            for i in range(10)
        ]
    ).astype(np.float32)
    o1 = cluster_order(vecs, 0.9)
    o2 = cluster_order(vecs.copy(), 0.9)
    assert o1 == o2, "cluster order must be deterministic"
    assert sorted(o1) == list(range(10)), "must be a permutation"
    evens, odds = {0, 2, 4, 6, 8}, {1, 3, 5, 7, 9}
    first_half = set(o1[:5])
    assert first_half in (evens, odds), f"clusters must be contiguous, got {o1}"
    print("test_cluster_order_deterministic: OK")


def main() -> None:
    test_cluster_order_deterministic()
    if not HAVE:
        print("SKIP: ffmpeg/ssimulacra2/cjxl/djxl not all present (brew install jpeg-xl)")
        return
    with tempfile.TemporaryDirectory(prefix="nest-qgate-") as tmp:
        base = Path(tmp)
        test_gate_structure_and_choice(base)
        test_visual_floor_rejects(base)
        test_drift_floor_rejects_alone(base)
        test_jxl_transcode_roundtrip(base)
    print("all quality gate tests passed")


if __name__ == "__main__":
    main()
