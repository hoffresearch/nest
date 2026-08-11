# user guide

operating notes for ai agents and human contributors working in this repo. the principal author writes fast and uses voice transcription. typos, caps lock, missing accents are common. read intent, do not flag tone, do not project emotional risk.

# build and test

- `cargo build --workspace` / `cargo build --release --workspace`
- `cargo test --workspace`: all rust tests (unit + integration + golden), 340/340 on the current unreleased state (`forge-core` adds 6 more on its own manifest)
- `cargo fmt --all --check`: formatting check
- `cargo clippy --workspace --all-targets -- -D warnings`: linting (warnings are errors)
- `ruff check .` / `ruff format --check .`: python linting and formatting (config in `pyproject.toml`)
- `./scripts/release_check.sh`: full pipeline + regression gates against `dat/measure/baseline.json`. single source of truth for "PR-ready". exits non-zero on any failure.
- `forge-core` (the ingestion layer) is a SEPARATE cargo workspace OUTSIDE `crates/`; the sovereign `--workspace` commands and `release_check.sh` do not touch it. build and test it on its own manifest: `cargo build --manifest-path forge-core/Cargo.toml`, `cargo test --manifest-path forge-core/Cargo.toml`, `cargo clippy --manifest-path forge-core/Cargo.toml --all-targets -- -D warnings`, `cargo fmt --manifest-path forge-core/Cargo.toml --all --check`. the <=300-line rule applies there too (release_check's guard only scans `crates/`).

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
python tests/test_offline_guard.py
python tests/test_blob_bridge.py
python tests/test_space_bridge.py
python tests/test_image_corpus.py
```

no pytest. tests are plain scripts with `if __name__ == "__main__"`. `pytest tests/` does not work. `test_image_corpus.py` (37 cases) exercises the forge image-corpus pillar (encode/decode, gop probe, sharding, ordering) and skips cleanly when ffmpeg's av1/avif encoders are unavailable; it does not need the vision model itself. building an actual image corpus (`python/forge/embed_image.py`) needs `open_clip` + torch, not part of the default forge dependency group.

the forge build-side default embedder is now the REAL SEMANTIC one: a vendored model2vec/potion-base-8M static table (`python/forge/embed_potion.py`), offline, no torch, no network. its self-test needs numpy + tokenizers and the vendored table (git-lfs): `python python/forge/test_embed_potion.py` (run with `.venv/bin/python`; install the deps with `uv pip install numpy tokenizers`, declared in `pyproject.toml` under `[dependency-groups] forge`). it proves the semantic jump (car ~ automobile >> car ~ banana), determinism, f32-stability, and that no socket is opened at embed time; `python python/forge/recall_harness.py` shows per-query recall vs the floor. the table (`python/forge/models/potion-base-8M/model.safetensors`, ~30mb) is git-lfs; run `git lfs pull` if it is a pointer. the #04 lexical bag-of-words stays as the stdlib-only zero-dep FLOOR with its own self-test (no `.so`, no deps): `python python/forge/test_embed_default.py`. both self-fingerprint to a `model_hash` recorded in provenance; neither is run by `release_check.sh`.

# single-target commands

- `cargo test -p nest-format`: format crate only
- `cargo test -p nest-runtime`: runtime crate only
- `cargo test --release -p nest-runtime --test hnsw_recall`: HNSW recall regression (needs release; debug is too slow to run within timeout)
- `cargo test -p nest-cli`: CLI integration tests (requires release build)
- `cargo run -p nest-format --example regen_golden`: regenerate the byte-frozen golden fixture

# architecture

```
nest-format  standalone library (binary format spec, reader, writer, manifest, encoding, hashing)
nest-runtime depends on nest-format (mmap-backed search, MmapNestFile, ann::HnswIndex, bm25::Bm25Index, graph::CsrIndex, simd dispatcher)
nest-cli     depends on nest-format + nest-runtime (clap binary, 9 engine subcommands + the ask/retrieve flagship verbs)
nest-python  depends on nest-format + nest-runtime (cdylib _nest, PyO3 abi3-py312)

forge-core   SEPARATE cargo workspace at the repo root, OUTSIDE crates/ (ingestion layer,
             FORGE-0a: the frozen .fci canonical-intermediate schema). its deps never enter the
             sovereign crates; not in the `--workspace` set. .fci is versioned independently.
```

CLI binary: `nest`. nine engine subcommands: `inspect`, `validate`, `search`, `search-ann`, `search-graph`, `search-text`, `benchmark`, `stats`, `cite`. plus two agent-native flagship verbs layered over the same engine: `ask` (text query in, cited answer out, `--disclose answer|explain`) and `retrieve` (json/jsonl answer-pack of cited spans where score IS the exact rerank value). the flagship keeps the nine subcommands as-is under the hood; verb-collapse, the `nest dev` namespace, and the nest-profile crate are deferred (churn with no user value pre-users).

python entry: `sys.path.insert(0, "python"); import nest`. dynamic loader finds `_nest.so` or `lib_nest.dylib`.

# repo workflow

- remote: `git@github.com:hoffresearch/nest.git`. owner: hoff research. maintainer: brenner cruvinel (`brenner@hoffresearch.com`).
- branches: `main` is release; `dev` is integration. work happens in `dev` (or feature branches off `dev`).
- PRs target `dev` from feature branches. release PRs target `main` from `dev`. squash merge into `main` to keep history linear.
- tags on `main` only (`v0.2.0` is current). `Cargo.toml` workspace version tracks the latest released tag.
- LFS: `dat/corpus_next.v1.nest` is tracked via Git LFS.
- demo datasets under `dat/demo/` are intentionally gitignored and downloaded locally from upstream sources listed in `dat/demo/README.md`.
- tests run without the demo datasets (the unit and golden-fixture tests avoid depend on them); only `measure_presets.py` and `release_check.sh` need the baseline corpus.
- `dat/measure/corpus_*.nest` and `*.nest-*` are gitignored: regeneration artifacts, not assets. the JSON files next to them ARE tracked (regression baselines).

# conventions

- every change ships with real tests, no mocks: happy path, error path, one edge case minimum.
- test against real artifacts (built .nest files, golden fixtures, real corpora), never mocked interfaces.
- applies to every contributor, human or agent; nothing merges without executable proof.
- keep the doc/changelog.md test-surface count in sync when adding suites.

# naming

directories, docs, and assets: kebab-case in english. source files: idiomatic to the language. check folder structure consistency with the project's design pattern. when proposing renames or moves, list them as `mv` commands. after renaming, test every import touched by the change and fix them. run the test suite after the operation.

build folders, files, and codebase items following apl-style 3-char tokens that read as variable syntax. that keeps the glyph atomic and context natural. high core value for neurodivergent readers.

# docs

write in diataxis style. all lowercase. no emojis. no em-dash. no decorative markdown. pragmatic, professional, objective. every doc starts with a yaml header for semantic resolution (helps llm, agentic, vector search): project, audience, status, last-updated, domain. design notes that turn out wrong get a note on top. they are not deleted.

architecture references live as a pair under `doc/arc/`:

- `doc/arc/arc.yaml` is the single architecture reference: machine-readable for agents and tooling, and the human-readable inventory plus contract narrative (system_view, contract, quality, risks, inventory).
- `doc/arc/arc.mmd` is the visual architecture map (mermaid).

at task start, read `doc/arc/arc.yaml` and `doc/arc/arc.mmd` in a short pass to preserve structure and naming pattern. after any implementation, refactor, rename, or doc move that changes architecture, boundaries, data flow, module layout, public contracts, storage, or runtime behavior, update `arc.yaml` and `arc.mmd` in the same change. keep them concise and pragmatic. do not keep a parallel second architecture document.
# file hygiene

hard limit is 333 lines per file. operational target for new files is 220 lines. human working memory holds 4 plus or minus 1 chunks at once (cowan 2001, refining miller). neural networks also work better that way. a file that does not fit the "mental window" forces internal context switching, degrading comprehension and raising bug rates. this is unnecessary cognitive load, the same principle applied in ux.

every file created or modified in a session that exceeds 333 lines must be read in full and refactored along single-responsibility lines. the rust source carve-out (`crates/**/src/**` at 300 lines) and the test-file exemptions documented below remain in force.

# audit when finishing a task 

run a full audit over every change made in the session, no summarizing, from devops, code quality, and secops angles. write a temporary manifest in markdown under your tmp folder to track tasks executed.

identify every trace of dead code, generated scripts and files no longer useful, items needing update, and items to be moved to the correct location per architecture and design pattern. if the project lacks documented conventions, create them: design notes in `doc/changelog.md` for architectural decisions, `.editorconfig` for stack-agnostic base formatting, and an idiomatic linter config per language used.

identify temporary scripts and possible dead-code files in incorrect folders. understand how each works, preserve application integrity, test and validate that no imports or responsibilities are left orphan. run tests after execution.

- rust edition 2024, resolver 3, `thiserror` for errors (never panic in library code).
- `repr(C)` structs for binary layout; all integers LE unsigned.
- Wip feature study/progress (not final resolutiom) binary format v1 is frozen. v0.2 added encodings 1/2/3 (zstd, float16, int8) and optional sections 0x07 (HNSW) and 0x08 (BM25). v0.3 added encoding 7 (int4) and the graph pillar (section 0x0C). since then the media blob pillar (0x14 blob_refs, 0x16 blob_span_overlay) and the multimodal space pillar (0x15 space_table + the 0x20-0x2F embedding band) shipped, all additive and content_hash-excluded; see `doc/arc/arc.yaml`'s `contract` array for the full section-id map.
- hash format: always `sha256:<64 lowercase hex>`.
- four hashes: `header_checksum`, per-section `checksum` (physical bytes), `file_hash` (whole file), `content_hash` (decoded canonical sections, stable across encodings).
- `NestFileBuilder` is a consuming builder (`add_chunk(self) -> Self`). presets via `.text_encoding()` + `.embedding_dtype()`.
- matryoshka prefix truncation is a build-time kwarg (`nest.build(mrl_dim=K)` / `BuildConfig.mrl_dim`): the python builder slices each l2-normalized row to its first K components and re-l2-normalizes the prefix BEFORE quantization, sets the header/manifest `embedding_dim` to K, and records the source dim as `full_dim`. additive optional manifest fields (`mrl_dim`/`full_dim`, omitted when unset so existing files stay byte-identical). NO runtime kernel change: the reader strides by `header.embedding_dim`. int4 needs the EFFECTIVE dim %64==0, so the int4 ladder is valid only at mrl_dim in {256,192,128}. truncation is a pure deterministic slice => byte-identical builds; content_hash is over the truncated embeddings so citations are tied to a given mrl_dim. the shipped MiniLM corpus is NOT mrl-trained, so truncation costs measured recall: `measure_presets.py --variants mrl<DIM>-<dtype>` reports the curve, gated conditionally in `compare_measure.py`.
- HNSW build is deterministic given a seed. BM25 index is sorted by alphabetical term order.
- `model_hash` is a granular fingerprint over `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`. zero-placeholder is rejected at write time.
- runtime SIMD dispatch: AVX2 (x86_64), NEON (aarch64), scalar fallback. `NEST_FORCE_SCALAR=1` forces scalar for A/B benchmarks.
- file hygiene: every rust source file in `crates/**/src/**` and every first-party python module is at most 300 lines. test files and the `crates/nest-format/tests/roundtrip.rs` carve-out are exempt.
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
- **NEON f16 MSRV**: `float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94, but the workspace MSRV is 1.85 (`rust-version` in the workspace `Cargo.toml` — the single msrv source; clippy reads it too). `crates/nest-runtime/build.rs` probes the compiling rustc and emits `cfg(neon_f16)` at >= 1.94; that cfg gates `simd/neon.rs::dot_f32_f16_neon` and its dispatch arm, and older toolchains fall back to the scalar f16 kernel. the kernel carries `#[clippy::msrv = "1.94"]` to match the cfg guarantee. avoid remove build.rs or the cfg gate without bumping the workspace `rust-version` to >= 1.94.
- **HNSW recall test needs release mode**: debug is 30x slower and hits the 60s default cargo test timeout. always run with `--release`.
- **PT-BR fingerprint corpus**: the model fingerprint is computed against the local sentence-transformers cache. first-time builders must `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"` to populate the cache, otherwise `nest_build_corpus.py` and the fingerprint test fail.
- **squash merge breaks `dev` history**: when a PR squash-merges into `main`, the squashed commit hash differs from the originals on `dev`. subsequent merges of `main` into `dev` will conflict on any files the squash touched. resolve by `git checkout --ours` from `dev` (dev is always the source of truth post-squash; main is just a flat snapshot).
- **avoid run `cargo clean` casually**: rebuild times are 30-60s for the full workspace. incremental compilation handles most edits.
- **`ask`/`retrieve` embed OFFLINE with potion, NOT sentence-transformers**: the flagship verbs shell out to `python/forge/embed_query_potion.py` (the default potion static table, numpy + tokenizers, no torch, no socket), so they stay offline-by-construction. only `search-text` uses `python/embed_query.py` (sentence-transformers, network on first use). the embedder runs under `python3` unless `NEST_PYTHON` is set; point `NEST_PYTHON` at a venv that carries the forge deps (numpy + tokenizers + the git-lfs potion table) or the embed step fails with `ModuleNotFoundError`. the two flagship e2e tests in `cli_e2e.rs` and `python/forge/test_retrieve.py` need those deps and skip cleanly when absent; they are not run by `release_check.sh`.
- **`cite` is tier-1 only**: it returns the stored canonical text + verifying hashes, NEVER an original-byte reopen. `ask`/`retrieve` print the same tier-1 text. do not let help text or docs claim original-byte reopen (that is net-new tier-2 catalog work, post-gate).

# known gaps 

these are documented honest limitations of the current code, not bugs to silently fix. user-visible behavior; flag them in any work that interacts with these areas.

- **`search-text` boot overhead (~300-500ms)**: each invocation forks a python process, imports sentence-transformers, embeds the query, then exits. the latency table in the README and `doc/usage.md` measures the search path AFTER the vector is ready, not end-to-end. python-driven workloads (`nest.NestFile.search` in a loop) avoid this.
- **BM25 tokenizer is word-segmented-only**: `crates/nest-runtime/src/bm25/tokenize.rs` splits on non-alphanumeric Unicode boundaries. correct for latin, cyrillic, greek, devanagari. degrades for CJK, thai, lao (each character becomes a token, posting lists explode, recall drops). hybrid search on those languages should disable BM25 (`with_bm25=False`) until a language-aware tokenizer ships.
- **no PyPI / maturin**: distribution is manual `cargo build` + `cp .dylib`. fine for the current audience (engineers embedding into a pipeline), real friction for casual adopters. maturin + PyPI publish is on the v0.3 backlog.
- **the semantic default embedder is english**: `potion-base-8M` is distilled from `bge-base-en-v1.5`, so english synonyms cluster tightly (car ~ automobile +0.78 vs car ~ banana +0.04) but non-english text rides english subword rows and the semantic signal is weak (carro ~ automovel +0.08 vs carro ~ banana -0.05: right direction, small margin). for a primarily non-english corpus, bring a multilingual sentence-transformers model (the ceiling path) or a multilingual potion table. the lexical floor is language-agnostic but captures literal token overlap only.

# things to avoid 

- **avoid write markdown that wasn't requested**. 
- **avoid bump `NEST_FORMAT_VERSION` for additive changes**. encodings 4-255 and section IDs 0x09+ are reserved within v1. v2 only when an existing field changes meaning.
- **avoid `--no-verify` git hooks** unless explicitly asked.
- **avoid force-push `main` ever**. force-push `dev` only after explicit user confirmation. squash-merge from PR is fine because that goes through GitHub.
- **avoid run `git add -A`** in repos that may carry untracked secrets or LFS payloads. stage explicit paths.
- **avoid bypass `release_check.sh`**. if it fails, fix the underlying issue. suppressing a clippy lint is fine when justified inline (`#[allow(clippy::name)]` + comment); suppressing the whole gate is not.
- **avoid introduce `unsafe` without a `// SAFETY:` comment** that names the invariant the caller is relying on.
- **avoid add em-dashes or emoji** to project files. consistency check in CI is informal but the maintainer reads diffs.

# documentation

- `README.md`: project overview, install, CLI summary, presets, v0.2 highlights, embedded mermaid system view.
- `doc/arc/arc.yaml`: the single architecture reference, machine-readable for agents and tooling and the human-readable inventory plus runtime contract summary.
- `doc/arc/arc.mmd`: mermaid sequence diagram of the build and query flows.
- `doc/usage.md`: how-to for the nine engine subcommands plus the ask/retrieve flagship verbs, presets, offline mode, citations.
- `doc/changelog.md`: v0.1.0, v0.2.0, and unreleased deltas.
- `dat/demo/README.md`: what each upstream PT-BR dataset is and how to rebuild the unified corpus.
- `CONTRIBUTING.md`: external contributor flow.
- `CODE_OF_CONDUCT.md`: contributor covenant 2.1, lowercase plain-style.
- `scripts/release_check.sh`: read it. it documents the gate by being the gate.

# agent instructions

this file is the single instruction source for ai coding agents: use/update/init only .contracts/.agents/AGENTS.md (the core global agent file). codex and most agentic tooling already read .contracts/.agents/AGENTS.md by default; point claude, gemini, cursor rules and similar tools here on init. do not create CLAUDE.md, GEMINI.md, CODEX.md, or any parallel instruction doc.
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
