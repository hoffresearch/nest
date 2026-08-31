"""Embedding cache with the RFC-0 N2 key triad and N8 concurrency rules.

A cache entry is valid only when (model_hash, embedding_recipe_hash,
corpus_input_hash) all match — row keys alone are never a key. Writes go to a
temp file + atomic rename under an flock'd lockfile; a sha256 sidecar guards
against torn writes: any mismatch means recompute, never reuse.
"""

from __future__ import annotations

import fcntl
import hashlib
import json
import os
import tempfile
from contextlib import contextmanager
from pathlib import Path

import numpy as np

TRIAD_KEYS = ("model_hash", "embedding_recipe_hash", "corpus_input_hash")


@contextmanager
def locked(path: Path):
    lock_path = path.with_suffix(path.suffix + ".lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("w") as fh:
        fcntl.flock(fh, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(fh, fcntl.LOCK_UN)


def atomic_write_bytes(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".")
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def atomic_write_json(path: Path, payload: dict) -> None:
    atomic_write_bytes(path, json.dumps(payload, sort_keys=True, indent=1).encode())


def _sidecar(path: Path) -> Path:
    return path.with_suffix(path.suffix + ".sha256")


class EmbedCache:
    """One .npz per (corpus, model preset), guarded by the triad."""

    def __init__(self, cache_dir: Path, corpus_name: str, preset: str):
        self.path = Path(cache_dir) / f"{corpus_name}.{preset}.npz"

    def load(self, triad: dict) -> dict[str, np.ndarray] | None:
        """Return cached arrays iff the triad and the checksum both match."""
        with locked(self.path):
            if not (self.path.is_file() and _sidecar(self.path).is_file()):
                return None
            digest = hashlib.sha256(self.path.read_bytes()).hexdigest()
            if _sidecar(self.path).read_text().strip() != digest:
                return None  # torn write: recompute
            with np.load(self.path, allow_pickle=False) as z:
                meta = json.loads(bytes(z["meta"]).decode())
                if any(meta.get(k) != triad[k] for k in TRIAD_KEYS):
                    return None
                return {k: z[k] for k in z.files if k != "meta"}

    def store(self, triad: dict, arrays: dict[str, np.ndarray]) -> None:
        meta = np.frombuffer(
            json.dumps({k: triad[k] for k in TRIAD_KEYS}, sort_keys=True).encode(), dtype=np.uint8
        )
        with locked(self.path):
            fd, tmp = tempfile.mkstemp(dir=self.path.parent, suffix=".npz")
            os.close(fd)
            try:
                np.savez(tmp, meta=meta, **arrays)
                os.replace(tmp, self.path)
            finally:
                if os.path.exists(tmp):
                    os.unlink(tmp)
            digest = hashlib.sha256(self.path.read_bytes()).hexdigest()
            atomic_write_bytes(_sidecar(self.path), digest.encode())


def canonical_hash(payload: dict) -> str:
    """sha256 over canonical JSON — the same convention as model fingerprints."""
    blob = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(blob.encode()).hexdigest()
