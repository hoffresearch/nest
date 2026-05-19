"""Python entry point for the .nest binary format.

Loads the PyO3 extension `_nest` (built from the `nest-python` Rust crate)
and re-exports a stable surface:

  - nest.open(path)                     -> NestFile
  - NestFile.search(query, k)           -> list[SearchHit] (exact, recall=1.0)
  - NestFile.search_ann(query, k, ef)   -> list[SearchHit] (HNSW + exact rerank)
  - NestFile.search_hybrid(query, query_text, k, candidates) -> list[SearchHit]
  - NestFile.embedding_dim
  - NestFile.n_embeddings
  - NestFile.dtype                       ("float32" | "float16" | "int8")
  - NestFile.simd_backend                ("scalar" | "avx2" | "neon")
  - NestFile.has_ann / has_bm25
  - NestFile.file_hash / content_hash
  - SearchHit fields: chunk_id, score, score_type, source_uri,
    offset_start, offset_end, embedding_model, index_type, reranked,
    file_hash, content_hash, citation_id
  - nest.build(..., preset=...)         -> path
  - nest.chunk_id(text, source_uri, byte_start, byte_end, chunker_version)
"""
import importlib.util
import os


def _find_extension() -> str | None:
    base = os.path.dirname(os.path.abspath(__file__))
    for name in ("_nest.so", "_nest.dylib", "lib_nest.dylib"):
        candidate = os.path.join(base, name)
        if os.path.exists(candidate):
            return candidate
    return None


_ext_path = _find_extension()
if _ext_path is None:
    raise ImportError(
        "Cannot find _nest extension. Run "
        "`cargo build --release -p nest-python && cp target/release/lib_nest.dylib python/_nest.so` "
        "from the repo root."
    )

_spec = importlib.util.spec_from_file_location("_nest", _ext_path)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)

NestFile = _mod.NestFile
SearchHit = _mod.SearchHitPy
build = _mod.build
chunk_id = _mod.chunk_id


def open(path: str):
    """Open a .nest file for read-only mmap-backed search."""
    return NestFile.open(path)


__all__ = ["NestFile", "SearchHit", "open", "build", "chunk_id"]
