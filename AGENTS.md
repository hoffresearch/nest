# build and test

- `cargo build --workspace` / `cargo build --release --workspace`
- `cargo test --workspace`: all rust tests (unit + integration + golden), 134/134 in v0.2
- `cargo fmt --all --check`: formatting check
- `cargo clippy --workspace --all-targets -- -D warnings`: linting (warnings are errors)
- `ruff check .` / `ruff format --check .`: python linting and formatting (config in `pyproject.toml`)
- `./scripts/release_check.sh`: full pipeline + regression gates against `data/measure/baseline.json`

# pyo3 extension

no maturin. build manually:

```
cargo build --release -p nest-python
cp target/release/lib_nest.dylib python/_nest.so   # macOS
cp target/release/lib_nest.so   python/_nest.so    # linux
```

abi3 targets python 3.12+ (not 3.14). python tests need the built `.so` first:

```
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
```

no pytest. tests are plain scripts with `if __name__ == "__main__"`.

# single-target commands

- `cargo test -p nest-format`: format crate only
- `cargo test -p nest-runtime`: runtime crate only
- `cargo test -p nest-runtime --test hnsw_recall`: HNSW recall regression (needs release)
- `cargo test -p nest-cli`: CLI integration tests (requires release build)
- `cargo run -p nest-format --example regen_golden`: regenerate the byte-frozen golden fixture

# architecture

```
nest-format  standalone library (binary format spec, reader, writer, manifest, encoding, hashing)
nest-runtime depends on nest-format (mmap-backed search, MmapNestFile, ann::HnswIndex, bm25::Bm25Index, simd dispatcher)
nest-cli     depends on nest-format + nest-runtime (clap binary, 8 subcommands)
nest-python  depends on nest-format + nest-runtime (cdylib _nest, PyO3 abi3-py312)
```

CLI binary: `nest`. subcommands: `inspect`, `validate`, `search`, `search-ann`, `search-text`, `benchmark`, `stats`, `cite`.

python entry: `sys.path.insert(0, "python"); import nest`. dynamic loader finds `_nest.so` or `lib_nest.dylib`.

# conventions

- rust edition 2024, resolver 3, `thiserror` for errors (never panic in library code).
- `repr(C)` structs for binary layout; all integers LE unsigned.
- binary format v1 is frozen. v0.2 added encodings 1/2/3 (zstd, float16, int8) and optional sections 0x07 (HNSW) and 0x08 (BM25), all within v1. bump `NEST_FORMAT_VERSION` for breaking changes.
- hash format: always `sha256:<64 lowercase hex>`.
- four hashes: `header_checksum`, per-section `checksum` (physical bytes), `file_hash` (whole file), `content_hash` (decoded canonical sections, stable across encodings).
- `NestFileBuilder` is a consuming builder (`add_chunk(self) -> Self`). presets via `.text_encoding()` + `.embedding_dtype()`.
- HNSW build is deterministic given a seed. BM25 index is sorted by alphabetical term order.
- `model_hash` is a granular fingerprint over `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`. zero-placeholder is rejected at write time. see ADR 0008.
- runtime SIMD dispatch: AVX2 (x86_64), NEON (aarch64), scalar fallback. `NEST_FORCE_SCALAR=1` forces scalar for A/B benchmarks.
- file hygiene: every rust source file in `crates/**/src/**` and every first-party python module is at most 300 lines. test files exempt. see ADR 0011.
- golden fixture: `crates/nest-format/tests/fixtures/golden_v1_minimal.nest` (1366 bytes, byte-frozen).
- CLI `search` takes JSON f32 array positional arg; `search-text` shells out to `python/embed_query.py` and validates the embedder's `model_hash` against the manifest.

# documentation

- `README.md`: project overview, install, CLI summary, presets, v0.2 highlights.
- `doc/architecture.md`: binary layout, API surface, errors, versioning.
- `doc/usage.md`: 10-section how-to for the 8 commands, presets, offline mode, citations.
- `doc/changelog.md`: v0.1.0 and v0.2.0 deltas.
- `doc/spec.md`: byte-by-byte format spec.
- `kdb/adr/`: 11 architectural decision records (0001 through 0011).
- `database/README.md`: what each upstream PT-BR dataset is and how to rebuild the unified corpus.
