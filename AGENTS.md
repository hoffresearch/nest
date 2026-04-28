# build and test

- `cargo build --workspace` / `cargo build --release --workspace`
- `cargo test --workspace`: all rust tests (unit + integration + golden), 134/134 in v0.2
- `cargo fmt --all --check`: formatting check
- `cargo clippy --workspace --all-targets -- -D warnings`: linting (warnings are errors)
- `ruff check .` / `ruff format --check .`: python linting and formatting (config in `pyproject.toml`)
- `./scripts/release_check.sh`: full pipeline + regression gates against `data/measure/baseline.json`. single source of truth for "PR-ready". exits non-zero on any failure.

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

no pytest. tests are plain scripts with `if __name__ == "__main__"`. `pytest tests/` does not work.

# single-target commands

- `cargo test -p nest-format`: format crate only
- `cargo test -p nest-runtime`: runtime crate only
- `cargo test --release -p nest-runtime --test hnsw_recall`: HNSW recall regression (needs release; debug is too slow to run within timeout)
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

# repo workflow

- remote: `git@github.com:hoffresearch/nest.git`. owner: hoff research. maintainer: brenner cruvinel (`brenner@hoffresearch.com`).
- branches: `main` is release; `dev` is integration. work happens in `dev` (or feature branches off `dev`).
- PRs target `dev` from feature branches. release PRs target `main` from `dev`. squash merge into `main` to keep history linear.
- tags on `main` only (`v0.2.0` is current). `Cargo.toml` workspace version tracks the latest released tag.
- LFS: `database/` and `data/corpus_next.v1.nest` are tracked via Git LFS. first checkout pulls ~600 MB. tests run without LFS (the unit and golden-fixture tests avoid depend on it); only `measure_presets.py` and `release_check.sh` need the corpus.
- `data/measure/corpus_*.nest` and `*.nest-*` are gitignored: regeneration artifacts, not assets. the JSON files next to them ARE tracked (regression baselines).

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
- file hygiene: every rust source file in `crates/**/src/**` and every first-party python module is at most 300 lines. test files and the `crates/nest-format/tests/roundtrip.rs` carve-out are exempt. see ADR 0011.
- golden fixture: `crates/nest-format/tests/fixtures/golden_v1_minimal.nest` (1366 bytes, byte-frozen).
- CLI `search` takes JSON f32 array positional arg; `search-text` shells out to `python/embed_query.py` and validates the embedder's `model_hash` against the manifest.

# style

documentation, comments, and commit messages follow the README's tone.

- lowercase headers throughout markdown (acronyms like `## CLI` are the only exception).
- no em-dash (`—`). use `,` `;` `.` or a regular hyphen `-`.
- no emoji.
- short paragraphs, direct voice, no marketing copy.
- commit messages in plain english, no Conventional Commits prefix. body explains the why; the diff already shows the what.

# gotchas

- **rebuild `python/_nest.so` after every rust change** that touches `nest-format`, `nest-runtime`, or `nest-python`. python tests load it via `dlopen`; stale `.so` will pass tests against old code. `release_check.sh` does this for you; manual workflows must remember.
- **NEON f16 MSRV**: `float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94, but workspace MSRV in `clippy.toml` is 1.85. `crates/nest-runtime/src/simd/neon.rs::dot_f32_f16_neon` carries `#[allow(clippy::incompatible_msrv)]` for that reason. avoid remove the allow without bumping MSRV in `clippy.toml`.
- **HNSW recall test needs release mode**: debug is 30x slower and hits the 60s default cargo test timeout. always run with `--release`.
- **PT-BR fingerprint corpus**: the model fingerprint is computed against the local sentence-transformers cache. first-time builders must `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"` to populate the cache, otherwise `nest_build_corpus.py` and the fingerprint test fail.
- **squash merge breaks `dev` history**: when a PR squash-merges into `main`, the squashed commit hash differs from the originals on `dev`. subsequent merges of `main` into `dev` will conflict on any files the squash touched. resolve by `git checkout --ours` from `dev` (dev is always the source of truth post-squash; main is just a flat snapshot).
- **avoid run `cargo clean` casually**: rebuild times are 30-60s for the full workspace. incremental compilation handles most edits.

# known gaps

these are documented honest limitations of the current code, not bugs to silently fix. user-visible behavior; flag them in any work that interacts with these areas.

- **`search-text` boot overhead (~300-500ms)**: each invocation forks a python process, imports sentence-transformers, embeds the query, then exits. the latency table in the README and `doc/usage.md` measures the search path AFTER the vector is ready, not end-to-end. python-driven workloads (`nest.NestFile.search` in a loop) avoid this.
- **BM25 tokenizer is word-segmented-only**: `crates/nest-runtime/src/bm25/tokenize.rs` splits on non-alphanumeric Unicode boundaries. correct for latin, cyrillic, greek, devanagari. degrades for CJK, thai, lao (each character becomes a token, posting lists explode, recall drops). hybrid search on those languages should disable BM25 (`with_bm25=False`) until a language-aware tokenizer ships.
- **no PyPI / maturin**: distribution is manual `cargo build` + `cp .dylib`. fine for the current audience (engineers embedding into a pipeline), real friction for casual adopters. maturin + PyPI publish is on the v0.3 backlog.

# things not to do

- **avoid write markdown that wasn't requested**. 
- **avoid bump `NEST_FORMAT_VERSION` for additive changes**. encodings 4-255 and section IDs 0x09+ are reserved within v1. v2 only when an existing field changes meaning.
- **avoid `--no-verify` git hooks** unless explicitly asked.
- **avoid force-push `main` ever**. force-push `dev` only after explicit user confirmation. squash-merge from PR is fine because that goes through GitHub.
- **avoid run `git add -A`** in repos that may carry untracked secrets or LFS payloads. stage explicit paths.
- **avoid bypass `release_check.sh`**. if it fails, fix the underlying issue. suppressing a clippy lint is fine when justified inline (`#[allow(clippy::name)]` + comment); suppressing the whole gate is not.
- **avoid introduce `unsafe` without a `// SAFETY:` comment** that names the invariant the caller is relying on.
- **avoid add em-dashes or emoji** to project files. consistency check in CI is informal but the maintainer reads diffs.

# documentation

- `README.md`: project overview, install, CLI summary, presets, v0.2 highlights.
- `doc/architecture.md`: binary layout, API surface, errors, versioning, four hashes, SIMD dispatcher, search contract.
- `doc/usage.md`: 10-section how-to for the 8 commands, presets, offline mode, citations.
- `doc/changelog.md`: v0.1.0 and v0.2.0 deltas.
- `doc/spec.md`: byte-by-byte format spec, the canonical reference for any external implementer.
- `kdb/adr/`: 11 architectural decision records (0001 through 0011). every irreversible decision lives here.
- `database/README.md`: what each upstream PT-BR dataset is and how to rebuild the unified corpus.
- `CONTRIBUTING.md`: external contributor flow.
- `CODE_OF_CONDUCT.md`: contributor covenant 2.1, lowercase plain-style.
- `scripts/release_check.sh`: read it. it documents the gate by being the gate.
