diga # nest

sovereign embedded vector database. a single `.nest` file carries chunks, embeddings, source spans, optional HNSW/BM25/chunk-graph indices, and a search contract, all hash-verified and memory-mapped. python builds, rust serves, the `.nest` file is the only artifact that crosses between them. there is no server: queries are answered from mmap, offline by construction.

this file is the entry summary for ai coding agents. the authoritative operating contract (workflow, gotchas, known gaps, things to avoid) lives in `.contracts/.agents/AGENTS.md` and is the single agent instruction source; read it before non-trivial work. the architecture source of truth is the pair under `doc/arc/`: `arc.yaml` (human and machine reference), `arc.mmd` (mermaid).

## tech stack

- rust workspace, edition 2024, resolver 3, MSRV 1.85 (`rust-version` in the root `Cargo.toml` is the single MSRV source).
- python 3.12+ tooling layer, imported by `sys.path.insert(0, "python")`; no package is published and there is no pip/poetry install path.
- PyO3 bridge (`nest-python`, cdylib, abi3-py312) built manually with cargo, no maturin.
- key deps: memmap2, rayon, zstd, half, bytemuck, sha2, thiserror, clap, serde/serde_json.
- `pyproject.toml` exists only to give ruff a canonical config plus the optional `forge` dependency group (numpy + tokenizers for the vendored potion embedder). install with `uv pip install numpy tokenizers` or `uv sync --group forge`.

## repository layout

```
crates/nest-format    frozen v1 container: layout, manifest, sections, encodings, hashes, reader, writer
crates/nest-runtime   mmap open, SIMD dispatcher, exact/ann/graph/hybrid search with mandatory exact rerank
crates/nest-cli       thin clap binary `nest`: 9 engine subcommands + the ask/retrieve flagship verbs
crates/nest-python    PyO3 bridge exposing the runtime to python (`_nest.so`)
python/               writer pipeline (builder.py), model fingerprint, query embedders, forge/ tools
forge-core/           SEPARATE cargo workspace (ingestion layer, frozen .fci schema), outside crates/
tests/                python test scripts (plain scripts, not pytest)
doc/                  arc/ architecture trio, usage.md, changelog.md, data-governance.md
dat/                  corpus_next.v1.nest (LFS demo corpus), measure/ regression baselines, demo/ sources
scripts/              release_check.sh (the merge gate), pre-commit (PHI/data backstop hook)
.contracts/.agents/   AGENTS.md, the single agent instruction source
```

dependency direction: `nest-format` is standalone; `nest-runtime` depends on it; `nest-cli` and `nest-python` depend on both. `forge-core` never enters the sovereign workspace: its (eventually heavy, possibly non-deterministic) ingestion deps stay out of the format/runtime crates.

## build and test commands

rust workspace:

```
cargo build --workspace            # debug
cargo build --release --workspace  # release
cargo test --release --workspace   # all rust tests (unit + integration + golden)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # warnings are errors
```

single-target:

```
cargo test -p nest-format
cargo test -p nest-runtime
cargo test --release -p nest-runtime --test hnsw_recall   # release required, debug times out
cargo test -p nest-cli
cargo run -p nest-format --example regen_golden           # regenerate byte-frozen golden fixture
```

PyO3 extension (required before any python test; no maturin):

```
cargo build --release -p nest-python
cp target/release/lib_nest.dylib python/_nest.so   # macOS
cp target/release/lib_nest.so   python/_nest.so    # linux
```

python tests are plain scripts with `if __name__ == "__main__"`; `pytest tests/` does not work:

```
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
```

forge-core has its own manifest and its own gate, untouched by `--workspace` and `release_check.sh`:

```
cargo build --manifest-path forge-core/Cargo.toml
cargo test  --manifest-path forge-core/Cargo.toml
```

the merge gate is `./scripts/release_check.sh`: release build, tests, clippy, fmt, the 300-line guard, `.so` rebuild, python tests, ruff, then `measure_presets.py` + `compare_measure.py` regression gates against `dat/measure/baseline.json`. it exits non-zero on any failure and takes 2-3 minutes warm. do not bypass it. env knobs: `NEST_BASELINE`, `NEST_QUERIES`, `NEST_K`, `NEST_PYTHON`, `NEST_OUT`.

python lint: `ruff check .` / `ruff format --check .` (config in `pyproject.toml`, line-length 100, rules E,F,W,I,B,UP,SIM).

## architecture and runtime contract

build flow (python, offline, reproducible): `builder.py` chunks, embeds, fingerprints the model, and calls `nest.build(...)`, which emits a deterministic container with four hashes. query flow (rust, offline, mmap): the runtime opens the file, validates hashes, checks the embedder's `model_hash` against the manifest, gathers hnsw/bm25/graph candidates, and reranks with mandatory exact cosine, so every reported `score` is real cosine, never an ANN proxy.

- format v1 is frozen. new encodings and sections are additive within v1 (encodings 4-255, section ids 0x09+ are reserved); bump `NEST_FORMAT_VERSION` only when an existing field changes meaning.
- four hashes: `header_checksum`, per-section `checksum` (physical bytes), `file_hash` (whole file), `content_hash` (decoded canonical sections, stable across encodings). hash format is always `sha256:<64 lowercase hex>`.
- same chunks + same model fingerprint + `reproducible=True` produce byte-identical files; that is what makes the `nest://content_hash/chunk_id` citation URI point at content, not at a copy.
- `cite`, `ask`, and `retrieve` are tier-1 only: they return the stored canonical text plus verifying hashes, never an original-byte reopen.
- `model_hash` fingerprints `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`; a mismatch between runtime model and corpus model fails loudly with a typed error.
- SIMD dispatch: AVX2 on x86_64, NEON on aarch64, scalar fallback; accumulators are always f32. `NEST_FORCE_SCALAR=1` forces scalar for A/B benchmarks. the NEON f16 kernel is gated by `build.rs` emitting `cfg(neon_f16)` at rustc >= 1.94 (MSRV is 1.85); do not remove that gate without bumping `rust-version`.
- presets bundle the levers: `exact`, `compressed` (zstd + f16), `tiny` (int8 + hnsw), `micro` (mrl256-int8), `nano` (int4 block-64), `hybrid` (f32 + hnsw + bm25). `mrl_dim=K` is a build-time matryoshka slice-then-renormalize lever; int4 needs the effective dim divisible by 64.
- CLI: `ask`, `retrieve`, `inspect`, `validate`, `stats`, `search`, `search-ann`, `search-graph`, `search-text`, `benchmark`, `cite`. `ask`/`retrieve` embed offline with the vendored potion static table (`python/forge/embed_query_potion.py`, numpy + tokenizers, no torch, no socket); only `search-text` uses sentence-transformers (`python/embed_query.py`, network on first use). embedders run under `python3` unless `NEST_PYTHON` points at a venv with the forge deps.
- python api: `nest.open(path)` returns a `NestFile` with `search`, `search_ann`, `search_hybrid`, `retrieve`, `validate`, `inspect`. hits carry `citation_id`, `source_uri`, offsets, and the exact-rerank `score`.

## conventions

- real tests only, no mocks: happy path, error path, one edge case minimum, against real artifacts (built `.nest` files, golden fixtures, real corpora). keep the `doc/changelog.md` test-surface count in sync when adding suites.
- no panic in library code; typed errors via `thiserror`. `repr(C)` structs for binary layout, all integers little-endian unsigned.
- every `unsafe` block needs a `// SAFETY:` comment naming the invariant.
- deterministic behavior for hashing and index builds is a hard requirement: HNSW build is seed-deterministic, BM25 is sorted by alphabetical term order.
- file hygiene: hard limit 333 lines per file, target 220 for new files. rust sources under `crates/**/src/**` and first-party python modules are capped at 300 lines (enforced by release_check for rust); test files and `crates/nest-format/tests/roundtrip.rs` are exempt.
- naming: directories/docs/assets in kebab-case english; source files idiomatic to the language.
- docs follow diataxis, all lowercase, no emoji, no em-dash, short paragraphs, yaml header (project, audience, status, last-updated, domain). commit messages are plain english without Conventional Commits prefixes; the body explains the why.
- after any change to architecture, boundaries, data flow, module layout, or public contracts, update `doc/arc/arc.yaml` and `arc.mmd` in the same change.
- base formatting via `.editorconfig`: utf-8, lf, 4-space indent (2 for toml/yaml/json), final newline.

## git and release flow

- remote `git@github.com:hoffresearch/nest.git`. `main` is release, `dev` is integration; PRs target `dev`, release PRs target `main` from `dev`, squash-merged. tags on `main` only (`v0.2.0` current; the workspace `Cargo.toml` version tracks the latest tag).
- git lfs tracks `*.nest`, `*.safetensors`, datasets, and the vendored potion table; golden fixtures under `crates/nest-format/tests/fixtures/` stay in regular git. run `git lfs pull` if a binary is a pointer.
- `dat/demo/` datasets are local-only and gitignored; `dat/measure/corpus_*.nest` and `*.nest-*` are regeneration artifacts (gitignored) while the JSON baselines next to them are tracked.
- `scripts/pre-commit` is a PHI/data backstop that aborts commits staging non-allow-listed data artifacts; install per clone with `cp scripts/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit` (copy, not `core.hooksPath`, so git-lfs hooks keep working).
- never force-push `main`; avoid `git add -A` (stage explicit paths); avoid `--no-verify` on hooks.

## gotchas

- rebuild `python/_nest.so` after every rust change touching `nest-format`, `nest-runtime`, or `nest-python`; python tests `dlopen` it and a stale `.so` passes against old code. `release_check.sh` does this automatically (and pins `PYO3_PYTHON` to the test interpreter to avoid segfaults).
- the HNSW recall test needs release mode; debug is ~30x slower and hits the test timeout.
- the PT-BR corpus build needs the sentence-transformers cache populated first: `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"`.
- unit and golden tests run without the demo datasets; only `measure_presets.py` and `release_check.sh` need the baseline corpus.
- avoid casual `cargo clean`; full workspace rebuilds take 30-60s.
- after a squash merge into `main`, merging `main` back into `dev` conflicts on squashed files; resolve with `git checkout --ours` from `dev` (dev is the source of truth).

## known gaps (documented limitations, not bugs)

- `search-text` forks a python embedder per call (~300-500ms boot overhead); latency numbers measure the search path after the vector is ready.
- the BM25 tokenizer is word-segmented-only; it degrades on CJK/thai/lao, so hybrid search on those languages should disable BM25.
- the vendored potion embedder is english-distilled; non-english semantic signal is weak, bring a multilingual model for non-english corpora.
- published `recall@10` figures use a self-perturbation ruler (rank stability under quantization, likely inflated vs real-query quality); the `ruler` field in `dat/measure/ladder.json` records this.
- no PyPI/maturin distribution yet; install is manual `cargo build` + `cp`.

## references

- `README.md`: overview, install, CLI, presets, v0.2 highlights.
- `.contracts/.agents/AGENTS.md`: the operating contract and single agent instruction source.
- `doc/arc/arc.yaml` / `arc.mmd`: architecture pair, source of truth.
- `doc/usage.md`: engine subcommands, flagship verbs, presets, offline mode, citations.
- `doc/changelog.md`: v0.1.0, v0.2.0, unreleased deltas.
- `dat/demo/README.md`: upstream datasets and corpus rebuild.
- `CONTRIBUTING.md`: external contributor flow.
- license: MIT.
