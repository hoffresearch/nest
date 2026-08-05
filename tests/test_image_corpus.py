#!/usr/bin/env python3
"""End-to-end test for image dataset -> .nest corpus.

Requires the optional forge image deps (torch, open_clip, Pillow) and ffmpeg
for the compressed path. The test skips cleanly if they are missing.

It uses a tiny sample of the glyfos dataset because the images are small and
publicly present in the repo's dat area.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))

GLYFOS_DIR = Path("/Volumes/HOFF/dat/glyfos-graph/multiscript_glyph_images_v1")


class ImageCorpusTest(unittest.TestCase):
    def setUp(self):
        try:
            import open_clip  # noqa: F401
            import torch  # noqa: F401
        except ImportError:
            self.skipTest("torch/open_clip not available")
        if not GLYFOS_DIR.exists():
            self.skipTest("glyfos dataset not present")
        self.tmp = Path(tempfile.mkdtemp(prefix="nest-image-test-"))

    def tearDown(self):
        import shutil

        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run_build(self, compress: bool) -> Path:
        from tools import nest_build_image_corpus

        output = self.tmp / f"glyfos{'-compressed' if compress else ''}.nest"
        result = nest_build_image_corpus.build_corpus(
            input_dir=GLYFOS_DIR,
            output_path=output,
            dataset_name="glyfos-test",
            embedder=nest_build_image_corpus.embed_image.ImageEmbedder(
                model_id="hf-hub:redlessone/DermLIP_ViT-B-16",
                batch_size=8,
            ),
            compress=compress,
            video_dir=self.tmp / "videos",
            sample=30,
            preset="compressed",
        )
        self.assertTrue(output.exists())
        self.assertEqual(result["n_images"], 30)
        return output

    def test_build_uncompressed(self):
        output = self._run_build(compress=False)
        import nest

        db = nest.open(str(output))
        self.assertEqual(db.n_embeddings, 30)

    def test_build_compressed(self):
        import shutil

        if not shutil.which("ffmpeg"):
            self.skipTest("ffmpeg not available")
        output = self._run_build(compress=True)
        import nest

        db = nest.open(str(output))
        self.assertEqual(db.n_embeddings, 30)

    def test_search(self):
        output = self._run_build(compress=False)
        images = sorted(
            p
            for p in GLYFOS_DIR.rglob("*")
            if p.is_file() and p.suffix.lower() in (".png", ".jpg", ".jpeg")
        )
        if len(images) < 5:
            self.skipTest("too few glyfos images")
        query = images[0]

        from forge import embed_image
        from tools import nest_search_image

        embedder = embed_image.ImageEmbedder(
            model_id="hf-hub:redlessone/DermLIP_ViT-B-16", batch_size=8
        )
        qvec = embedder.embed_single(query).tolist()

        db = nest_search_image.nest.open(str(output))
        hits = db.search(qvec, 5)
        self.assertTrue(len(hits) > 0)
        # The top hit is the query itself only when the embedder is domain-matched.
        # For the generic pipeline test we just assert the hit is well-formed.
        self.assertGreater(hits[0].score, 0.5)
        self.assertTrue(hits[0].citation_id.startswith("nest://"))


if __name__ == "__main__":
    unittest.main()
