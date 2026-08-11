# user guide

operating notes for ai agents and human contributors working in this repo. the principal author writes fast and uses voice transcription. typos, caps lock, missing accents are common. read intent, do not flag tone, do not project emotional risk.

# build and test

- `cargo build --workspace` / `cargo build --release --workspace`
- `cargo test --workspace`: all rust tests (unit + integration + golden), 288/288 on the current unreleased state (`forge-core` adds 6 more on its own manifest)
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
```

no pytest. tests are plain scripts with `if __name__ == "__main__"`. `pytest tests/` does not work.

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
- Wip feature study/progress (not final resolutiom) binary format v1 is frozen. v0.2 added encodings 1/2/3 (zstd, float16, int8) and optional sections 0x07 (HNSW) and 0x08 (BM25).
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
