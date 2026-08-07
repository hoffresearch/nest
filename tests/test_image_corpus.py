#!/usr/bin/env python3
"""End-to-end tests for the image corpus pipeline.

These run against a dataset generated inside the test, not against a path
on the author's disk: a test that skips everywhere except one laptop is not
coverage. The vision model is deliberately NOT used here. The invariants
worth guarding are the pipeline's (frame alignment, cache correctness,
portability, the model_hash gate), and none of them need 600 MB of
checkpoint to prove. A stub embedder that reduces pixels to a small
deterministic vector exercises every one of them.

Needs ffmpeg with libsvtav1 for the compressed cases; those skip cleanly
when it is absent. Run: python tests/test_image_corpus.py
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))

import numpy as np

STUB_DIM = 64


def have_ffmpeg() -> bool:
    if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
        return False
    out = subprocess.run(["ffmpeg", "-hide_banner", "-encoders"], capture_output=True, text=True)
    return "libsvtav1" in out.stdout


class StubEmbedder:
    """Deterministic 64-dim embedder: 8x8 grayscale, flattened, normalized.

    Crude, but a real function of the pixels, so neighbourhood structure
    survives compression the way a learned embedder's would, and identical
    input provably yields identical vectors.
    """

    model_id = "stub-pixel-8x8"
    batch_size = 8

    def __init__(self, salt: str = ""):
        self.salt = salt

    @property
    def dim(self) -> int:
        return STUB_DIM

    @property
    def model_hash(self) -> str:
        payload = f"{self.model_id}|{STUB_DIM}|{self.salt}"
        return "sha256:" + hashlib.sha256(payload.encode()).hexdigest()

    def _vector(self, image) -> np.ndarray:
        small = image.convert("L").resize((8, 8))
        vec = np.asarray(small, dtype=np.float32).reshape(-1)
        norm = float(np.linalg.norm(vec))
        return vec / norm if norm else np.ones(STUB_DIM, dtype=np.float32) / np.sqrt(STUB_DIM)

    def embed_paths(self, paths) -> np.ndarray:
        from PIL import Image

        out = []
        for path in paths:
            with Image.open(path) as img:
                out.append(self._vector(img))
        return np.vstack(out)

    def embed_arrays(self, frames) -> np.ndarray:
        from PIL import Image

        return np.vstack([self._vector(Image.fromarray(arr)) for arr in frames])

    def embed_one(self, path) -> np.ndarray:
        return self.embed_paths([path])[0]


def make_dataset(root: Path, count: int = 12) -> Path:
    """Distinct, codec-survivable images: large flat colour blocks.

    Noise would be destroyed by any lossy codec and would make the test
    measure the codec's noise handling instead of the pipeline.
    """
    from PIL import Image, ImageDraw

    root.mkdir(parents=True, exist_ok=True)
    for i in range(count):
        hue = (i * 97) % 256
        img = Image.new("RGB", (320, 240), (hue, (hue * 3) % 256, (255 - hue)))
        draw = ImageDraw.Draw(img)
        # a per-image block so the 8x8 reduction separates them
        draw.rectangle([20 + (i % 4) * 60, 20 + (i % 3) * 50, 120 + (i % 4) * 60, 140], fill=0)
        img.save(root / f"img{i:03d}.png")
    return root


class ImageCorpusTest(unittest.TestCase):
    def setUp(self):
        try:
            import PIL  # noqa: F401
        except ImportError:
            self.skipTest("Pillow not available")
        self.tmp = Path(tempfile.mkdtemp(prefix="nest-image-test-"))
        self.src = make_dataset(self.tmp / "src")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _build(
        self,
        name: str,
        *,
        compress: bool,
        scratch_db=None,
        labels=None,
        dataset=None,
        sample=None,
        all_intra=False,
    ) -> dict:
        from tools import nest_build_image_corpus as builder

        return builder.build_corpus(
            all_intra=all_intra,
            input_dir=self.src,
            output_path=self.tmp / name / f"{name}.nest",
            dataset_name=dataset or name,
            embedder=StubEmbedder(),
            compress=compress,
            labels=labels,
            scratch_db=scratch_db,
            sample=sample,
            width=256,
        )

    # ---- frame alignment: the bug class that silently shifts every uri ----

    def test_compressed_frames_align_with_items(self):
        if not have_ffmpeg():
            self.skipTest("ffmpeg with libsvtav1 not available")
        import nest

        result = self._build("aligned", compress=True)
        media = result["media"]
        self.assertEqual(media["frame_count"], result["n_items"])
        self.assertTrue(media["media_sha256"].startswith("sha256:"))

        manifest = json.loads(Path(result["manifest"]).read_text())
        for item in manifest["items"]:
            self.assertEqual(item["source_uri"], f"media://aligned-av1.mp4#frame={item['ordinal']}")

        db = nest.open(result["nest"])
        self.assertEqual(db.n_embeddings, result["n_items"])
        db.validate()

    def test_all_intra_stays_frame_aligned(self):
        """keyint=1 changes the gop, so the alignment guard is re-checked.

        The lever exists because inter-frame prediction finds nothing on a
        corpus of unrelated images, but it must not buy size by losing a
        frame.
        """
        if not have_ffmpeg():
            self.skipTest("ffmpeg with libsvtav1 not available")
        import nest

        result = self._build("intra", compress=True, all_intra=True)
        self.assertEqual(result["media"]["keyint"], 1)
        self.assertEqual(result["media"]["frame_count"], result["n_items"])
        db = nest.open(result["nest"])
        self.assertEqual(db.n_embeddings, result["n_items"])
        db.validate()

    def test_probe_counts_every_encoded_frame(self):
        """The alignment guard leans on this count, so it is checked directly."""
        if not have_ffmpeg():
            self.skipTest("ffmpeg with libsvtav1 not available")
        from forge import image_media

        paths = sorted(self.src.glob("*.png"))
        canvas = image_media.canvas_size(paths, 256)
        out = self.tmp / "probe.mp4"
        info = image_media.encode_av1(paths, out, canvas=canvas)
        self.assertEqual(info["frame_count"], len(paths))
        self.assertEqual(image_media.probe_frame_count(out), len(paths))
        decoded = sum(len(b) for b in image_media.decode_frames(out, canvas, batch_size=5))
        self.assertEqual(decoded, len(paths), "decode yielded a different frame count")

    def test_sampling_renumbers_ordinals_densely(self):
        """A sampled corpus must renumber, not keep the original ordinals.

        The ordinal is also the frame number of the stream encoded from the
        sampled list. If sampling kept the source ordinals, item 7 of 12
        would claim `#frame=7` in a 6-frame stream: either a decode error or,
        worse, someone else's frame.
        """
        if not have_ffmpeg():
            self.skipTest("ffmpeg with libsvtav1 not available")
        from forge import image_media

        result = self._build("sampled", compress=True, sample=6)
        manifest = json.loads(Path(result["manifest"]).read_text())

        self.assertEqual(result["n_items"], 6)
        self.assertEqual(result["media"]["frame_count"], 6)
        self.assertEqual([i["ordinal"] for i in manifest["items"]], list(range(6)))
        for item in manifest["items"]:
            _, frame = image_media.parse_media_uri(item["source_uri"])
            self.assertEqual(frame, item["ordinal"])
        # the subset is drawn from the source and keeps the sorted order
        origins = [i["origin"] for i in manifest["items"]]
        self.assertEqual(origins, sorted(origins))
        self.assertEqual(len(set(origins)), 6)

    def test_sampling_is_seed_deterministic(self):
        """Same seed, same subset: otherwise a rebuild is a different corpus."""
        first = self._build("s1", compress=False, sample=6, dataset="sampletest")
        second = self._build("s2", compress=False, sample=6, dataset="sampletest")
        self.assertEqual(Path(first["nest"]).read_bytes(), Path(second["nest"]).read_bytes())

    # ---- the cache bug: a warm scratch db must not reshuffle vectors ----

    def test_partly_warm_cache_produces_identical_corpus(self):
        """A scattered cache miss set must still map to the right vectors.

        Pipeline hands the embedder ONLY the chunks the cache missed. If the
        builder answers by position instead of by chunk_id, every miss gets
        some other image's vector, and nothing anywhere reports an error: the
        corpus is simply wrong. Half the cache is dropped here so the miss
        set is scattered, which is the case a fully cold or fully warm run
        never reaches.
        """
        import sqlite3

        cache = str(self.tmp / "scratch.sqlite")
        # same dataset name on both, so the ONLY difference between the runs
        # is which vectors came from the cache.
        cold = self._build("cold", compress=False, scratch_db=cache, dataset="cachetest")
        # `with sqlite3.connect(...)` commits but does not close, so the
        # connection is closed explicitly rather than left to the finalizer.
        conn = sqlite3.connect(cache)
        try:
            with conn:
                conn.execute("DELETE FROM embeddings_v2 WHERE rowid % 2 = 0")
                remaining = conn.execute("SELECT count(*) FROM embeddings_v2").fetchone()[0]
        finally:
            conn.close()
        self.assertGreater(remaining, 0, "cache emptied; the test would prove nothing")

        warm = self._build("warm", compress=False, scratch_db=cache, dataset="cachetest")
        self.assertEqual(
            Path(cold["nest"]).read_bytes(),
            Path(warm["nest"]).read_bytes(),
            "partly-warm rebuild diverged: cached vectors were mismatched to chunks",
        )

    def test_search_finds_the_query_itself(self):
        import nest

        result = self._build("selftest", compress=False)
        manifest = json.loads(Path(result["manifest"]).read_text())
        target = manifest["items"][5]
        vec = StubEmbedder().embed_one(target["render_path"]).tolist()
        hits = nest.open(result["nest"]).search(vec, 3)
        self.assertEqual(hits[0].offset_start, target["ordinal"])

    # ---- portability: a corpus must survive being moved ----

    def test_corpus_is_relocatable(self):
        if not have_ffmpeg():
            self.skipTest("ffmpeg with libsvtav1 not available")
        from forge import image_media

        result = self._build("portable", compress=True)
        manifest = json.loads(Path(result["manifest"]).read_text())
        for item in manifest["items"]:
            self.assertNotIn("/", item["source_uri"].split("://")[1].split("#")[0])
            self.assertNotIn(str(self.tmp), item["source_uri"])

        moved = self.tmp / "elsewhere"
        moved.mkdir()
        shutil.copy(result["nest"], moved / "portable.nest")
        shutil.copytree(image_media.media_dir_for(Path(result["nest"])), moved / "portable.media")
        name, ordinal = image_media.parse_media_uri(manifest["items"][3]["source_uri"])
        frame = image_media.decode_frame(
            image_media.media_dir_for(moved / "portable.nest") / name,
            tuple(manifest["media"]["canvas"]),
            ordinal,
        )
        self.assertEqual(frame.shape, (*reversed(manifest["media"]["canvas"]), 3))

    # ---- the manifest gate has to hold for image corpora too ----

    def test_wrong_model_hash_is_rejected(self):
        import nest

        result = self._build("gated", compress=False)
        db = nest.open(result["nest"])
        vec = StubEmbedder().embed_one(sorted(self.src.glob("*.png"))[0]).tolist()
        db.retrieve(vec, 3, expected_model_hash=StubEmbedder().model_hash)
        with self.assertRaises(ValueError):
            db.retrieve(vec, 3, expected_model_hash=StubEmbedder(salt="other").model_hash)

    # ---- labels and canvas geometry ----

    def test_labels_reach_the_manifest(self):
        labels = {f"img{i:03d}": "benign" if i % 2 else "malignant" for i in range(12)}
        result = self._build("labelled", compress=False, labels=labels)
        manifest = json.loads(Path(result["manifest"]).read_text())
        self.assertEqual(manifest["items"][1]["label"], "benign")
        self.assertEqual(manifest["items"][0]["label"], "malignant")

    def test_pdf_pages_keep_their_provenance(self):
        """A pdf hit is only useful if the corpus can say which page it was.

        The rendered pages are build-time temporaries, so the durable path
        back to the pixels is the source pdf plus the page number. The eval
        harness has to be able to re-render from that.
        """
        try:
            import fitz
        except ImportError:
            self.skipTest("PyMuPDF not available")
        sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "tools"))
        import nest_image_eval as ev
        from tools import nest_build_image_corpus as builder

        pdf_dir = self.tmp / "pdfs"
        pdf_dir.mkdir()
        doc = fitz.open()
        for page in range(3):
            doc.new_page().insert_text((72, 144), f"page number {page}", fontsize=48)
        doc.save(str(pdf_dir / "guide.pdf"))
        doc.close()

        result = builder.build_corpus(
            input_dir=pdf_dir,
            output_path=self.tmp / "pdfcorpus" / "guide.nest",
            dataset_name="guide",
            embedder=StubEmbedder(),
            is_pdf=True,
            compress=False,
            width=256,
        )
        manifest = json.loads(Path(result["manifest"]).read_text())
        self.assertEqual(result["n_items"], 3)
        self.assertEqual(manifest["input_kind"], "pdf")
        self.assertEqual([i["page"] for i in manifest["items"]], [0, 1, 2])
        self.assertEqual(manifest["items"][2]["origin"], "guide.pdf")

        # the build-time render is gone; the harness must still find pixels.
        self.assertFalse(Path(manifest["items"][1]["render_path"]).exists())
        rendered = ev.query_image(manifest["items"][1], manifest, self.tmp / "requery")
        self.assertTrue(rendered.exists())

    def test_canvas_is_even_and_letterbox_does_not_distort(self):
        from forge import image_media
        from PIL import Image

        paths = sorted(self.src.glob("*.png"))
        canvas = image_media.canvas_size(paths, 256)
        self.assertEqual(canvas[0] % 2, 0)
        self.assertEqual(canvas[1] % 2, 0)
        # a wildly off-aspect image must be padded into the canvas, never
        # stretched to fill it.
        tall = Image.new("RGB", (40, 400), (255, 0, 0))
        padded = image_media.letterbox(tall, canvas)
        self.assertEqual(padded.size, canvas)
        corner = padded.getpixel((1, canvas[1] // 2))
        self.assertEqual(corner, (0, 0, 0), "expected padding, got stretched content")

    def test_delta_carries_a_confidence_interval(self):
        """A delta without an interval is not a result.

        The published claim "av1 costs 1.9 points of precision@10" came from a
        point estimate whose interval crossed zero: at n=200 that difference
        was not distinguishable from noise. The harness now reports the
        interval and says so, so the same claim cannot be made twice.
        """
        sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "tools"))
        import nest_image_eval as ev

        rng = np.random.default_rng(0)
        same = rng.normal(0.6, 0.2, 400)
        ci = ev.bootstrap_delta(same, same.copy(), seed=1)
        self.assertEqual(ci["mean"], 0.0)
        self.assertFalse(ci["significant"], "identical samples cannot differ")

        worse = np.clip(same - 0.30, 0, 1)
        ci = ev.bootstrap_delta(worse, same, seed=1)
        self.assertLess(ci["mean"], 0)
        self.assertLess(ci["ci95"][1], 0, "a 30-point drop must clear zero")
        self.assertTrue(ci["significant"])

        # a difference far below what this sample size resolves must NOT be
        # reported as real, which is the exact failure being guarded.
        tiny = same + rng.normal(0.002, 0.2, 400)
        self.assertFalse(ev.bootstrap_delta(tiny, same, seed=1)["significant"])

    def test_canvas_never_upscales_the_source(self):
        """`--width` is a ceiling, not a target.

        Upscaling invents no detail, so every interpolated pixel is bits the
        encoder spends on nothing and a decoder pays for on every read. On PH2
        the 1024 default sat above a 765-wide source and cost 33 percent of
        the file for zero information.
        """
        from forge import image_media

        paths = sorted(self.src.glob("*.png"))  # the fixture is 320x240
        canvas = image_media.canvas_size(paths, 4096)
        self.assertLessEqual(canvas[0], 320, "canvas widened past the source")
        self.assertLessEqual(canvas[1], 240)
        # below the source it still downscales, which is a real size lever.
        self.assertEqual(image_media.canvas_size(paths, 160)[0], 160)

    def test_canvas_cap_survives_mixed_source_sizes(self):
        """A few large images must not drag the whole canvas up.

        The cap follows the median source width for the same reason the
        aspect does: one outlier should not set the geometry for the corpus.
        """
        from forge import image_media
        from PIL import Image

        mixed = self.tmp / "mixed"
        mixed.mkdir()
        for i in range(9):
            Image.new("RGB", (200, 150), (i * 20, 0, 0)).save(mixed / f"s{i}.png")
        Image.new("RGB", (4000, 3000), (0, 0, 255)).save(mixed / "huge.png")
        canvas = image_media.canvas_size(sorted(mixed.glob("*.png")), 1024)
        self.assertLessEqual(canvas[0], 200, f"one outlier set the canvas: {canvas}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
