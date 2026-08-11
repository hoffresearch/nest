"""Python entry point for the .nest binary format.

Loads the PyO3 extension `_nest` (built from the `nest-python` Rust crate)
and re-exports a stable surface:

  - nest.open(path)                     -> NestFile
  - NestFile.search(query, k)           -> list[SearchHit] (exact, recall=1.0)
  - NestFile.search_ann(query, k, ef)   -> list[SearchHit] (HNSW + exact rerank)
  - NestFile.search_hybrid(query, query_text, k, candidates) -> list[SearchHit]
  - NestFile.retrieve(query, k, ...)    -> list[RetrieveHit] (agent-native:
        routes by manifest capability, score IS the exact-cosine rerank value,
        each hit carries the tier-1 stored canonical text + verifying hashes +
        the nest:// citation_id + the rerank_source precision marker. embed the
        query OFFLINE first; see python/forge/retrieve.py for the potion path.)
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


def _load_extension():
    """Load the `_nest` PyO3 extension.

    Two layouts share this file:
      - installed wheel: `nest` is a package and `_nest` is a proper
        submodule (`nest._nest`, named `_nest.abi3.so`), so a relative
        import resolves it.
      - dev repo: `nest.py` is a top-level module under `python/` and the
        extension sits next to it as `_nest.so` (see README > install);
        the relative import fails and we fall back to file-based loading.
    """
    try:
        from . import _nest

        return _nest
    except ImportError:
        pass
    base = os.path.dirname(os.path.abspath(__file__))
    for name in ("_nest.so", "_nest.abi3.so", "_nest.dylib", "lib_nest.dylib"):
        candidate = os.path.join(base, name)
        if os.path.exists(candidate):
            spec = importlib.util.spec_from_file_location("_nest", candidate)
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            return mod
    raise ImportError(
        "Cannot find _nest extension. Run "
        "`cargo build --release -p nest-python && "
        "cp target/release/lib_nest.dylib python/_nest.so` "
        "from the repo root."
    )


_mod = _load_extension()

NestFile = _mod.NestFile
SearchHit = _mod.SearchHitPy
RetrieveHit = _mod.RetrieveHitPy
build = _mod.build
chunk_id = _mod.chunk_id


def open(path: str):
    """Open a .nest file for read-only mmap-backed search."""
    return NestFile.open(path)


def potion_model_path() -> str | None:
    """Path to the bundled potion-base-8M model dir, or None.

    The wheel bundles the offline potion static table under
    `nest/models/potion-base-8M/` so `ask`/`retrieve`-style embedding works
    with no network after install. the dev repo keeps the table at
    `python/forge/models/potion-base-8M/` (git-lfs) and this returns None.
    """
    base = os.path.dirname(os.path.abspath(__file__))
    candidate = os.path.join(base, "models", "potion-base-8M")
    return candidate if os.path.isdir(candidate) else None


__all__ = [
    "NestFile",
    "SearchHit",
    "RetrieveHit",
    "open",
    "build",
    "chunk_id",
    "potion_model_path",
]
