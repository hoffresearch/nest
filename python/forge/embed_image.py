"""Vision embedder for image datasets.

This module lives in the forge tooling layer because it pulls optional
heavy dependencies (torch, open_clip, Pillow). It never enters the
sovereign nest runtime.

Supported models:
- `redlessone/DermLIP_ViT-B-16` for dermatology images.
- `ViT-B-16` or other open_clip models as a generic fallback.

The embedder exposes a model_hash so the resulting .nest files satisfy
the same manifest gate as text corpora.
"""

from __future__ import annotations

import hashlib
import os
from collections.abc import Sequence
from pathlib import Path

import numpy as np


def _compute_model_hash(model_id: str, files_hash: str, dim: int, normalize: bool) -> str:
    payload = f"{model_id}\n{files_hash}\n{dim}\n{normalize}"
    return "sha256:" + hashlib.sha256(payload.encode("utf-8")).hexdigest()


class ImageEmbedder:
    """Embed images with a vision model.

    Loads the model lazily on first embed call so callers that only need
    the model metadata do not pay the import cost.
    """

    def __init__(
        self,
        model_id: str = "hf-hub:redlessone/DermLIP_ViT-B-16",
        *,
        pretrained: str | None = None,
        device: str | None = None,
        batch_size: int = 32,
    ):
        self.model_id = model_id
        self.pretrained = pretrained
        self.device = device or self._default_device()
        self.batch_size = batch_size
        self._model = None
        self._preprocess = None
        self._dim: int | None = None

    @staticmethod
    def _default_device() -> str:
        try:
            import torch
        except ImportError:
            return "cpu"
        if torch.cuda.is_available():
            return "cuda"
        if torch.backends.mps.is_available():
            return "mps"
        return "cpu"

    def _load(self):
        if self._model is not None:
            return
        import open_clip
        import torch
        from PIL import Image

        # hf-hub:... models load their weights through the model_id itself.
        # Plain model names like ViT-B-32 require a pretrained tag.
        if self.model_id.startswith("hf-hub:"):
            model, _, preprocess = open_clip.create_model_and_transforms(self.model_id)
        else:
            tag = self.pretrained or "openai"
            model, _, preprocess = open_clip.create_model_and_transforms(
                self.model_id, pretrained=tag
            )
        self._model = model.to(self.device).eval()
        self._preprocess = preprocess
        self._dim = (
            model.output_dim
            if hasattr(model, "output_dim")
            else model.token_embedding.weight.shape[-1]
        )
        # open_clip models expose output_dim on the model.
        self._image_class = Image

    @property
    def dim(self) -> int:
        self._load()
        return self._dim  # type: ignore[return-value]

    @property
    def model_hash(self) -> str:
        self._load()
        # The files_hash here is a placeholder: open_clip caches are local and
        # vary by machine. For reproducible builds, callers should fingerprint
        # the actual local checkpoint files and pass files_hash explicitly.
        return _compute_model_hash(self.model_id, "open_clip_cached", self.dim, normalize=True)

    def embed(self, image_paths: Sequence[str | Path]) -> np.ndarray:
        """Return a float32 array of shape (n, dim), L2-normalized."""
        self._load()
        import torch

        embeddings = []
        paths = [Path(p) for p in image_paths]
        for i in range(0, len(paths), self.batch_size):
            batch_paths = paths[i : i + self.batch_size]
            tensors = []
            for p in batch_paths:
                img = self._image_class.open(p).convert("RGB")
                tensors.append(self._preprocess(img))
            batch = torch.stack(tensors).to(self.device)
            with torch.no_grad():
                feats = self._model.encode_image(batch)
                feats = feats / feats.norm(dim=-1, keepdim=True)
            embeddings.append(feats.cpu().numpy().astype(np.float32))
        return np.vstack(embeddings)

    def embed_single(self, image_path: str | Path) -> np.ndarray:
        """Return a single L2-normalized float32 vector."""
        return self.embed([image_path])[0]

    def embed_frames(self, frames: Sequence[np.ndarray]) -> np.ndarray:
        """Embed already-loaded RGB numpy arrays.

        Used when the searchable index must represent compressed video frames
        rather than the original pixel buffers.
        """
        self._load()
        import torch

        embeddings = []
        for i in range(0, len(frames), self.batch_size):
            batch = frames[i : i + self.batch_size]
            tensors = []
            for arr in batch:
                img = self._image_class.fromarray(arr).convert("RGB")
                tensors.append(self._preprocess(img))
            stacked = torch.stack(tensors).to(self.device)
            with torch.no_grad():
                feats = self._model.encode_image(stacked)
                feats = feats / feats.norm(dim=-1, keepdim=True)
            embeddings.append(feats.cpu().numpy().astype(np.float32))
        return np.vstack(embeddings)


def list_images(root: Path, extensions: Sequence[str] | None = None) -> list[Path]:
    if extensions is None:
        extensions = (".jpg", ".jpeg", ".png", ".bmp", ".webp", ".gif", ".tiff", ".tif")
    ext_set = {e.lower() for e in extensions}
    paths = [p for p in root.rglob("*") if p.is_file() and p.suffix.lower() in ext_set]
    paths.sort()
    return paths


def extract_frame_from_video(video_path: Path, frame_idx: int) -> np.ndarray:
    """Decode a single frame from a video file to an RGB numpy array."""
    import cv2

    cap = cv2.VideoCapture(str(video_path))
    cap.set(cv2.CAP_PROP_POS_FRAMES, frame_idx)
    ret, frame = cap.read()
    cap.release()
    if not ret:
        raise RuntimeError(f"could not decode frame {frame_idx} from {video_path}")
    return cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)


def model_fingerprint_from_local_cache(model_id: str, cache_dir: Path | None = None) -> str:
    """Hash the local open_clip/hf checkpoint files for reproducibility.

    This mirrors python/model_fingerprint.py for text embedders.
    """
    if cache_dir is None:
        cache_dir = Path.home() / ".cache" / "huggingface" / "hub"
    if not cache_dir.exists():
        return "sha256:" + "0" * 64
    # open_clip hub repos live under models--<org>--<repo> with a snapshots dir.
    # Hash all files under matching snapshots deterministically.
    digest = hashlib.sha256()
    # Best-effort: include files whose path contains a sanitized model id.
    token = model_id.replace("/", "--").replace(":", "--")
    found = False
    for p in sorted(cache_dir.rglob("*")):
        if p.is_file() and token in str(p):
            found = True
            digest.update(p.name.encode("utf-8"))
            digest.update(p.read_bytes())
    if not found:
        digest.update(b"no_local_cache")
    return "sha256:" + digest.hexdigest()
