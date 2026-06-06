"""Reusable Python pipeline for building deterministic .nest files.

The pipeline stages are:

  1. ingest documents (caller-provided)
  2. chunk text with byte-accurate spans
  3. compute or attach embeddings (caller-provided callable)
  4. cache (chunk_id -> embedding) in a SQLite scratch DB so re-runs
     skip the embedding step
  5. emit a .nest file via nest.build()
  6. invoke `nest validate` on the result

The chunker, embedder and emitter are deliberately decoupled so a caller
can swap the embedding model or the chunking strategy without touching
the rest of the pipeline.
"""

from __future__ import annotations

import json
import os
import sqlite3
import struct
import sys
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nest


@dataclass(frozen=True)
class ChunkSpec:
    """A pre-embedding chunk with the spans used to derive chunk_id."""

    canonical_text: str
    source_uri: str
    byte_start: int
    byte_end: int

    def chunk_id(self, chunker_version: str) -> str:
        return nest.chunk_id(
            self.canonical_text,
            self.source_uri,
            self.byte_start,
            self.byte_end,
            chunker_version,
        )


def chunk_text(
    text: str,
    source_uri: str,
    *,
    max_chars: int = 512,
    overlap: int = 0,
) -> list[ChunkSpec]:
    """Greedy character-window chunker. Splits on a hard character budget,
    with optional overlap. Returns chunks whose byte spans index into the
    UTF-8 encoding of `text`, so the spans round-trip through `nest cite`.

    The simplest possible thing that's still useful — production callers
    will want a sentence-aware splitter, but the chunk_id contract is
    independent of the splitter so it's easy to swap.
    """
    if max_chars <= 0:
        raise ValueError("max_chars must be > 0")
    if overlap < 0 or overlap >= max_chars:
        raise ValueError("overlap must be >= 0 and < max_chars")

    encoded = text.encode("utf-8")
    chunks: list[ChunkSpec] = []
    char_idx = 0
    text_len = len(text)
    while char_idx < text_len:
        end = min(char_idx + max_chars, text_len)
        piece = text[char_idx:end]
        prefix_bytes = len(text[:char_idx].encode("utf-8"))
        piece_bytes = len(piece.encode("utf-8"))
        chunks.append(
            ChunkSpec(
                canonical_text=piece,
                source_uri=source_uri,
                byte_start=prefix_bytes,
                byte_end=prefix_bytes + piece_bytes,
            )
        )
        if end == text_len:
            break
        char_idx = end - overlap
    # sanity: spans must cover bytes within the original encoding.
    for c in chunks:
        assert c.byte_end <= len(encoded), "byte span overshoots source"
    return chunks


class EmbeddingCache:
    """SQLite scratch keyed by chunk_id. Stores raw float32 embeddings.

    Intended to be reused across runs so re-builds skip the expensive
    embedding step for chunks that have already been computed.
    """

    SCHEMA = """
        CREATE TABLE IF NOT EXISTS embeddings (
            chunk_id TEXT PRIMARY KEY,
            dim INTEGER NOT NULL,
            data BLOB NOT NULL
        );
    """

    def __init__(self, path: str):
        self.path = path
        self.conn = sqlite3.connect(path)
        self.conn.executescript(self.SCHEMA)

    def get(self, chunk_id: str, dim: int) -> list[float] | None:
        row = self.conn.execute(
            "SELECT dim, data FROM embeddings WHERE chunk_id=?", (chunk_id,)
        ).fetchone()
        if row is None:
            return None
        if row[0] != dim:
            raise ValueError(f"chunk {chunk_id} cached with dim={row[0]}, requested dim={dim}")
        return list(struct.unpack(f"<{dim}f", row[1]))

    def put(self, chunk_id: str, embedding: Sequence[float]) -> None:
        data = struct.pack(f"<{len(embedding)}f", *embedding)
        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings(chunk_id, dim, data) VALUES(?,?,?)",
            (chunk_id, len(embedding), data),
        )
        self.conn.commit()

    def close(self) -> None:
        self.conn.close()


@dataclass
class BuildConfig:
    output_path: str
    embedding_model: str
    embedding_dim: int
    chunker_version: str
    model_hash: str
    title: str | None = None
    version: str | None = None
    description: str | None = None
    license: str | None = None
    reproducible: bool = True
    # encoding presets:
    #   "exact"      - raw text, float32 embeddings, no ANN, no BM25
    #   "compressed" - zstd text, float16 embeddings, no ANN, no BM25
    #   "tiny"       - zstd text, int8 embeddings, HNSW, no BM25
    #   "nano"       - zstd text, int4 block-64 embeddings, HNSW, no BM25
    #                  (first sub-int8 size lever; embedding_dim must be
    #                  divisible by 64; stored-precision cosine)
    #   "hybrid"     - zstd text, float32 embeddings, HNSW, BM25
    preset: str = "exact"
    # per-knob overrides (None = inherit from preset)
    text_encoding: str | None = None  # "raw" | "zstd"
    dtype: str | None = None  # "float32" | "float16" | "int8" | "int4"
    with_hnsw: bool | None = None
    with_bm25: bool | None = None
    hnsw_m: int = 16
    hnsw_ef_construction: int = 400
    hnsw_seed: int = 42


class Pipeline:
    """Glue between chunking, embedding, caching and the nest writer.

    Usage::

        pipe = Pipeline(cfg, embedder=embed_fn, scratch_db="cache.sqlite")
        for source_uri, text in documents:
            for spec in chunk_text(text, source_uri):
                pipe.add(spec)
        pipe.emit()
    """

    def __init__(
        self,
        cfg: BuildConfig,
        *,
        embedder: Callable[[Sequence[ChunkSpec]], list[Sequence[float]]],
        scratch_db: str | None = None,
    ):
        self.cfg = cfg
        self.embedder = embedder
        self.cache = EmbeddingCache(scratch_db) if scratch_db else None
        self._specs: list[ChunkSpec] = []

    def add(self, spec: ChunkSpec) -> None:
        self._specs.append(spec)

    def add_many(self, specs: Iterable[ChunkSpec]) -> None:
        self._specs.extend(specs)

    def _embeddings(self) -> list[list[float]]:
        # resolve from cache where possible; embed only the misses.
        cached: list[list[float] | None] = [None] * len(self._specs)
        misses_idx: list[int] = []
        misses: list[ChunkSpec] = []

        for i, spec in enumerate(self._specs):
            if self.cache is None:
                misses_idx.append(i)
                misses.append(spec)
                continue
            cid = spec.chunk_id(self.cfg.chunker_version)
            hit = self.cache.get(cid, self.cfg.embedding_dim)
            if hit is None:
                misses_idx.append(i)
                misses.append(spec)
            else:
                cached[i] = hit

        if misses:
            new_embs = self.embedder(misses)
            if len(new_embs) != len(misses):
                raise RuntimeError(
                    f"embedder returned {len(new_embs)} embeddings for {len(misses)} chunks"
                )
            for spec, idx, emb in zip(misses, misses_idx, new_embs, strict=False):
                if len(emb) != self.cfg.embedding_dim:
                    raise RuntimeError(
                        f"embedder produced dim={len(emb)}, expected {self.cfg.embedding_dim}"
                    )
                cached[idx] = list(emb)
                if self.cache is not None:
                    self.cache.put(spec.chunk_id(self.cfg.chunker_version), emb)

        # cached is fully populated by construction.
        return [c for c in cached if c is not None]  # type: ignore[misc]

    def emit(self, *, provenance: dict | None = None) -> str:
        if not self._specs:
            raise RuntimeError("pipeline has no chunks")
        embeddings = self._embeddings()

        chunks = [
            dict(
                canonical_text=s.canonical_text,
                source_uri=s.source_uri,
                byte_start=s.byte_start,
                byte_end=s.byte_end,
                embedding=emb,
            )
            for s, emb in zip(self._specs, embeddings, strict=False)
        ]

        if os.path.exists(self.cfg.output_path):
            os.unlink(self.cfg.output_path)
        nest.build(
            output_path=self.cfg.output_path,
            embedding_model=self.cfg.embedding_model,
            embedding_dim=self.cfg.embedding_dim,
            chunker_version=self.cfg.chunker_version,
            model_hash=self.cfg.model_hash,
            chunks=chunks,
            title=self.cfg.title,
            version=self.cfg.version,
            description=self.cfg.description,
            license=self.cfg.license,
            provenance=provenance,
            reproducible=self.cfg.reproducible,
            preset=self.cfg.preset,
            text_encoding=self.cfg.text_encoding,
            dtype=self.cfg.dtype,
            with_hnsw=self.cfg.with_hnsw,
            with_bm25=self.cfg.with_bm25,
            hnsw_m=self.cfg.hnsw_m,
            hnsw_ef_construction=self.cfg.hnsw_ef_construction,
            hnsw_seed=self.cfg.hnsw_seed,
        )

        # final integrity check via the in-process reader (PyO3 path).
        # no CLI subprocess: one Python entry point only.
        db = nest.open(self.cfg.output_path)
        db.validate()
        return self.cfg.output_path

    def close(self) -> None:
        if self.cache is not None:
            self.cache.close()
