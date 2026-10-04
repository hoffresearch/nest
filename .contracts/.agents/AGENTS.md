---
project: urna
audience: ai coding agents and human contributors
status: active
last-updated: 2026-10-03
domain: repo-ops
---

# Agents

The one instruction source for this repo. The root `CLAUDE.md` is a symlink to this file; never edit the link or add a parallel instruction file (GEMINI.md, CODEX.md, Cursor rules). Most agent tooling reads `.contracts/.agents/AGENTS.md` on its own; point the rest here on init.

The principal author writes fast and uses voice transcription: typos, caps lock and missing accents are common. Read the intent; do not flag tone or project emotional risk.

# Start here

1. Read `docs/arc/ARC.toml` in a short pass: the architecture, the file inventory, the build and query flows.
2. Work on a short-lived branch off `origin/main`, one pull request per topic, squash merged.
3. Every code change ships with real tests: happy path, error path, one edge case, against real artifacts (built `.urna` files, golden fixtures, real corpora), no mocks. No code merges without executable proof.
4. Before the pull request: `.contracts/.agents/.skills/AFTERWORK.md` (every doc the change owns, updated in place) and, for a code change, `./scripts/release_check.sh` (the gate); a docs-only change says in the pull request that the gate did not run.
5. The hard rules below protect three things: the frozen file format, the offline promise (no network stack in the binary, no socket at query time) and the release channels. Everything else is judgment. When a rule stands in the way of a better design, say so in the pull request and change the rule together with the change.

# Autonomy

On your own: branches, commits, pull requests, the test suites, workflows that publish nothing (CI, an `install-test` dispatch), installs into temporary prefixes, the docs a change owns.

Ask first, because it publishes or is hard to undo:

- Pushing or moving a tag. A `v*` tag publishes to crates.io, npm, Homebrew and PyPI, and a crates.io version is permanent.
- Merging past a blocked review (`--admin`): never. Wait for the maintainer.
- Force-push: `main` never (the ruleset blocks it); a feature branch only with `--force-with-lease` and an explicit ok.
- `git add -A`, `--no-verify`, deleting a remote branch, changing repository or organization settings, rotating a secret.

Secrets live in the GitHub repository secrets (`CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `HOMEBREW_TAP_TOKEN`) and the `pypi` environment. A token never goes into the tree, a commit, a log, a pull request or a chat message; one that did is compromised and gets revoked. The trusted-publisher path (OIDC) needs no token at all: prefer it where a registry offers it.

# Build and test

- `cargo build --workspace`, `cargo build --release --workspace`
- `cargo test --workspace`: every Rust test (unit, integration, golden); the count it prints is the one `docs/CHANGELOG` cites
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (warnings are errors)
- `cargo clippy -p urna --no-default-features --all-targets -- -D warnings`: the engine-only CLI, without the terminal UI, has to stay clean too; CI runs both
- `cargo deny check`: advisories, licenses (the allowlist in `deny.toml`), bans and sources over the lockfile. A yanked crate, a copyleft-only license or a git dependency fails CI; the same policy file covers the other two workspaces with `--manifest-path forge-core/Cargo.toml` and `--manifest-path fuzz/Cargo.toml`
- `cargo semver-checks -p urna-format --baseline-rev origin/main`: the Rust API of the frozen format against the pull request's base; CI fails a pull request that breaks it (in 0.x a minor bump is the major bump)
- `cargo bench -p urna-runtime --no-run`: the criterion benches (simd, rerank, hnsw_build) have to compile; CI checks that, the numbers are not a gate
- `sh scripts/ruff_check.sh`: ruff over the one Python file list shared with CI (`URNA_PYTHON=.venv/bin/python` picks the interpreter)
- `./scripts/release_check.sh`: the full pipeline (the Rust suite in release, the extension rebuilt, fourteen Python suites, ruff when importable) plus the regression gates against `data/measure/baseline.json`; exits non-zero on any failure. It is the definition of pull-request ready
- `forge-core/` is a separate cargo workspace outside `crates/` (the ingestion layer, the frozen `.fci` schema). `--workspace` and `release_check.sh` never reach it; run `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` with `--manifest-path forge-core/Cargo.toml`
- `fuzz/` is the third cargo workspace (cargo-fuzz, nightly toolchain): `sh scripts/fuzz_soak.sh [seconds]` runs every target with the corpus kept under `fuzz/corpus/`; `fuzz/README.md` has the targets and how a finding becomes a test

Single targets:

- `cargo test -p urna-format`, `cargo test -p urna-runtime`
- `cargo test --release -p urna-runtime --test hnsw_recall`: the recall regression; debug is 30x slower and hits the cargo test timeout
- `cargo test -p urna`: the CLI integration tests (cargo builds the binary they run, `CARGO_BIN_EXE_urna`)
- `cargo run -p urna-format --example regen_golden`: regenerate the byte-frozen golden fixture, only when the format really changed

## Python

The extension is built by hand in dev:

```
cargo build --release -p urna-python --features pyo3/extension-module
cp target/release/lib_urna.dylib python/_urna.so   # macos
cp target/release/lib_urna.so   python/_urna.so    # linux
```

`pyo3/extension-module` keeps libpython out of the cdylib; without it the `.so` hard-links a libpython path and segfaults under statically embedded interpreters (uv's python-build-standalone). `release_check.sh` builds with the feature and pins `PYO3_PYTHON` to the test interpreter.

The published wheel is maturin, staged; never edit `packaging/staging/` by hand:

```
python scripts/stage_wheel.py
(cd packaging/staging && maturin build --release)   # wheel lands in target/wheels/
```

`packaging/pyproject.toml` is the one source of the wheel project. The staging script copies it plus `python/urna.py` (as `urna/__init__.py`), `python/urna_cli.py` (as `urna/_cli.py`), `python/forge/embed_potion.py` and the potion table. abi3, Python 3.12 and up.

The wheel installs its own console script named `urna` (`python/urna_cli.py`): a read-only shim over the library API (`validate`, `inspect`, `stats`, `search`) so `uvx --from urna urna ...` works. It is not the Rust binary; `ask`, `retrieve`, `build`, `doctor` and the terminal UI exist only there. `pip install "urna[embed]"` adds numpy and tokenizers for `urna.embed_potion`; the core surface needs nothing beyond the wheel.

The Python tests are plain scripts with `if __name__ == "__main__"`; `pytest tests/` does not work. They need the built `.so` first:

```
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
python tests/test_offline_guard.py
python tests/test_blob_bridge.py
python tests/test_space_bridge.py
python tests/test_image_corpus.py
python tests/test_forge_spec.py
python tests/test_quality_gate.py
python tests/test_cli_space.py
python tests/test_query_embedder_routing.py
python tests/test_embedder_payload.py
python tests/test_bench_runner.py
python tests/test_model_catalog.py
python tests/test_model_install.py
python tests/test_release_preflight.py
python tests/test_pypi_release.py
```

`release_check.sh` runs fourteen of them; `test_offline_guard.py`, `test_blob_bridge.py` and `test_space_bridge.py` run by hand.

`test_image_corpus.py` covers the forge image pillar (encode and decode, GOP probe, sharding, ordering) with a stub embedder and skips cleanly without FFmpeg's AV1 and AVIF encoders. Building a real image corpus (`python/forge/embed_image.py`) needs `open_clip` and torch, outside the default forge dependency group.

The forge build-side default embedder is the vendored model2vec/potion-base-8M static table (`python/forge/embed_potion.py`): offline, no torch, no network.

- Its self-test is `python python/forge/test_embed_potion.py`. It needs numpy and tokenizers (`uv pip install numpy tokenizers`, the `forge` dependency group) and the table, which is git-lfs: `git lfs pull`, or `sh scripts/fetch_potion.sh` when LFS is unavailable.
- The self-test proves the semantic jump (car ~ automobile far above car ~ banana), determinism, f32 stability and that no socket opens at embed time. `python python/forge/recall_harness.py` shows per-query recall against the floor.
- The lexical bag-of-words floor (`python/forge/embed_default.py`) is stdlib-only, with its own self-test: `python python/forge/test_embed_default.py`.
- Both fingerprint to a `model_hash` recorded in provenance; neither runs in `release_check.sh`.
- Three more forge self-tests, also outside `release_check.sh`: `python python/forge/test_model_registry.py` (the registry contract, no ML deps), `python python/forge/test_embed_st.py` (the sentence-transformers worker against a local wemm-2b snapshot; skips without it) and `python python/forge/test_open_clip_snapshot.py` (the SigLIP2 query offline from its pinned snapshot alone, the build's `model_hash`, a missing file named; skips without torch, open_clip, transformers or the snapshot).

Python entry point: `sys.path.insert(0, "python"); import urna`. The loader finds `_urna.so` or `lib_urna.dylib`.

# Layout

```
crates/urna-format    frozen v1 container: layout, manifest, sections, encodings, hashes, reader, writer
crates/urna-runtime   mmap open, simd dispatch, hnsw, bm25, graph, exact/ann/graph/hybrid search with mandatory exact rerank
crates/urna-cli       the `urna` binary, published as the crate `urna`: engine verbs in cmd/*.rs, agent verbs in cmd/agent/*.rs,
                      the one model gate in cmd/embed_gate.rs, the terminal ui in src/tui (feature `tui`, on by default)
crates/urna-python    the pyo3 bridge, cdylib `_urna`, abi3-py312; ships as the wheel, not as a crate
forge-core/           separate cargo workspace: the frozen .fci canonical-intermediate schema of the ingestion layer
fuzz/                 separate cargo workspace (nightly, cargo-fuzz): four targets, seeds/, README.md; corpus/ is local, never committed
docker/               Dockerfile: the static musl binary in a scratch image; engine verbs only, no python inside
python/               the writer pipeline, model fingerprint, query embedders, and forge/ (declarative builds, model registry, quality gate)
tests/                python test scripts
data/                 the lfs demo corpus, measure/ regression baselines, demo/ sources (gitignored, see Instructions.md)
docs/                 arc/ARC.toml, USAGE.md, BENCH.md, CHANGELOG, SECURITY.md, CONTRIBUTING.md
scripts/              release_check.sh, ruff_check.sh, pre-commit, install.sh / install.ps1, fetch_potion.sh, stage_*.py
packaging/            pyproject.toml, the one source of the wheel (staging/ is generated)
examples/             quickstart (the five-verb loop on twelve cc0 paragraphs), fastapi, flask, jupyter
assets/images/        the readme header, the social thumbs, the setup and tui screenshots
```

The full map (every file, the flows, the contracts) is `docs/arc/ARC.toml`. Key Rust deps: memmap2, rayon, zstd, half, bytemuck, sha2, thiserror, clap, serde.

Three cargo workspaces, one policy each: the root `Cargo.toml` (`crates/*`, the one that ships), `forge-core/` and `fuzz/`. `deny.toml` is the cargo-deny policy for all three. `clippy.toml` pins cognitive complexity at 15 (Clippy's default is 25) and 7 arguments per function; a legitimate exception gets `#[allow(clippy::...)]` at the site, the threshold never moves. `rustfmt.toml` pins the stable defaults at width 100.

The CLI has three groups, which `urna --help` tags and orders:

- Engine verbs, file and vector in: `inspect`, `validate`, `stats`, `media`, `search`, `search-ann`, `search-graph`, `search-space`, `search-text`, `benchmark`, `cite`, `doctor`. Two of them run Python: `search-text` (the sentence-transformers embedder, `python/embed_query.py`) and `doctor` (it probes the Python env and runs one potion embed); the other ten never do.
- Agent verbs over the same engine, `cmd/agent/`: `build` (a declarative corpus build, launching `python/tools/urna_forge.py`), `ask` (text in, cited answer out, `--disclose answer|explain`), `retrieve` (JSON or JSONL of cited spans; `score` is the exact rerank value). They embed offline and route the query embedder by the manifest model: potion corpora keep the potion script, registry models go through `python/forge/embed_query_model.py`, which hands a sentence-transformers model no preset names to `python/embed_query.py`. The search path is routed by what the file carries (`MmapUrnaFile::search_routed`, shared with `search-text`): hybrid when a BM25 section is present, HNSW when an HNSW section is present, exact otherwise; the manifest `index_type` names the vector index only, so a `hybrid` preset file (`index_type = "hnsw"`, `supports_bm25 = true`) takes the hybrid route. The graph is reached only through `search-graph`. The build contract is `docs/USAGE.md` sections 12 to 14.
- The terminal UI, `src/tui`:
  - `urna setup` is the installer every channel ends in: scan the machine, show the plan, install, verify. The payload comes through a system `curl` child with the SHA256 checked while it streams; `--yes` for scripts; `--model <name>` (repeatable, `all`) also installs catalog models, `--allow-remote-code <name>` is the separate consent for a model's repo code. Exit codes: 10 download, 11 checksum, 12 unpack, 13 Python env, 14 blocked, 15 a model install, above doctor's 2 to 6.
  - `urna tui [file]` is the explorer: home, corpus, ask, health, a file picker, a hand-off to setup. A bare `urna` on a terminal opens it; in a pipe it prints help and exits 2.
  - `URNA_RELEASE_BASE=file:///dir` points setup at a local release (the e2e tests do this).

# Contract

The format and runtime invariants. A change that touches them needs the tests named next to each.

- Rust edition 2024, resolver 3, `thiserror` errors, never a panic in library code. `repr(C)` plus `bytemuck::Pod` for the binary layout, integers little-endian unsigned.
- Every `unsafe` block carries a `// SAFETY:` comment naming the invariant; Clippy denies undocumented ones and `unwrap` outside tests (`[workspace.lints]`). Prefer `bytemuck` casts over raw parts; keep raw-pointer kernels behind a safe dispatcher that asserts every length in release.
- Reading and bounds: fixed-width fields through `urna_format::bytes::{le_u32, le_u64, le_f32, array32}` (typed `UnexpectedEof`, never `try_into().unwrap()`); header-derived sizes with checked arithmetic; cursor checks as `need > remaining`, never `pos + need > len`; f32 scores sorted through the runtime's `order` module (`crates/urna-runtime/src/order.rs`, NaN-last total order), never `partial_cmp(..).unwrap_or(Equal)`.
- Binary format v1 is frozen. Encodings 4 to 255 and section ids 0x09 and up are reserved within v1 and additive; `URNA_FORMAT_VERSION` moves only when an existing field changes meaning. The section-id map is the `contract` array of `ARC.toml`. The ids the writer emits, none of them free for a new codec: encodings 1 zstd, 2 float16, 3 int8, 4 intpack, 5 zstd with a trained dictionary, 7 int4, 9 fsst, 10 txt_streams (`layout/mod.rs` names them all, 6 and 8 are reserved and unwritten); sections 0x07 hnsw, 0x08 bm25, 0x0A dictionary, 0x0B dedup map, 0x0C graph, 0x14 blob_refs, 0x15 space_table, 0x16 blob_span_overlay, 0x17 blob_data, and the 0x20 to 0x2F embedding band. The index, graph, media and space sections are content_hash-excluded; 0x0A and 0x0B are the text codecs' side tables and decode into the canonical text, which is inside it. Check `layout/mod.rs` before taking an id.
- A decoder change runs the mutation harness before the pull request: `cargo test -p urna-format --test mutation_fuzz -p urna-runtime --test mutation_fuzz` (`URNA_MUTATION_ITERS=25000` for a soak), then `sh scripts/fuzz_soak.sh` (nightly, cargo-fuzz). A new codec gets an arm in `fuzz/fuzz_targets/section_decoders.rs`; a finding becomes a `tests/negative_*.rs` before the fix and a `fuzz/seeds/regress-*.bin`.
- Four SHA-256 checks, two shapes. The two reported as `sha256:<64 lowercase hex>`: `file_hash` (the whole file, footer included; the footer itself stores the digest of everything before it, `[0, size-40)`) and `content_hash` (the decoded canonical sections, stable across encodings). The two stored raw in the layout: `header_checksum` and each section's `checksum` are the first 8 bytes of the SHA-256 over the physical bytes (`layout/header.rs`, `layout/section_entry.rs`; `inspect` prints them as 16 hex chars). Same chunks, same model fingerprint and `reproducible=True` give byte-identical files, so `urna://content_hash/chunk_id` points at content, not at a copy.
- `UrnaFileBuilder` is a consuming builder (`add_chunk(self) -> Self`). `preset=` takes `exact` (raw + f32), `compressed` (zstd + f16), `tiny` (zstd + int8 + hnsw), `nano` (zstd + int4 block 64 + hnsw) or `hybrid` (zstd + f32 + hnsw + bm25). `micro` in the tables is not a preset value: it is the published name for `tiny` at `mrl_dim=256`, built with `urna.build(text_encoding="zstd", dtype="int8", mrl_dim=256, with_hnsw=True)`.
- Matryoshka truncation is a build-time kwarg (`mrl_dim`): the Python builder slices each L2-normalized row to its first K components and re-normalizes before quantization; the header `embedding_dim` becomes K and `full_dim` records the source. No runtime kernel change.
  - int4 needs the effective dim divisible by 64, so its ladder is 256, 192, 128. content_hash covers the truncated vectors.
  - The shipped MiniLM corpus is not MRL-trained, so truncation costs measured recall (`measure_presets.py --variants mrl<DIM>-<dtype>`).
- HNSW builds are deterministic given a seed; the BM25 index is sorted by term.
- `model_hash` fingerprints `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`. `urna.build` (and `builder.Pipeline`) refuse the zero placeholder at write time unless `allow_placeholder_model_hash=True` (test fixtures); the Rust `UrnaFileBuilder` accepts any well-formed hash (the frozen golden fixture carries the placeholder), and the CLI gate refuses a placeholder corpus at query time. A runtime model that differs from the corpus model fails with a typed error.
- SIMD dispatch: AVX2 on x86_64, NEON on aarch64, scalar fallback, f32 accumulators. `URNA_FORCE_SCALAR=1` forces scalar.
- The golden fixture `crates/urna-format/tests/fixtures/golden_v1_minimal.urna` is byte-frozen at 1366 bytes.
- The reader also accepts the `NEST` magic of files written by 0.4.0, before the rename (`tests/legacy_magic.rs`, fixture `legacy_v040_minimal.nest`, same layout and hashes); the writer never emits it, and any other magic is rejected.
- CLI `search` takes a JSON f32 array; `search-text` shells out to `python/embed_query.py` and checks its `model_hash` against the manifest.
- Python API: `urna.open(path)` returns a `UrnaFile` with `search`, `search_ann`, `search_hybrid`, `search_graph`, `search_space`, `retrieve`, `validate`, `inspect`, plus `chunk_ids()` (identity in file order, the key to match hits under any media ordering), `blob_refs()`, `has_blobs` and `blob_bytes(i)` (one inlined blob sliced off the mmap); hits carry `citation_id`, `source_uri`, offsets and the exact-rerank `score`.
- `cite` is tier-1 only: the stored canonical text plus the verifying hashes, never an original-byte reopen. `ask` and `retrieve` print the same text. No help text or doc claims otherwise.
- The binary links no network stack and the runtime never opens a socket. Setup downloads through the system `curl`.

# Workflow

- Remote `git@github.com:hoffresearch/urna.git`, owner Hoff Research, maintainer Brenner Cruvinel (`brenner@hoffresearch.com`).
- `main` is the only long-lived branch. The ruleset requires pull requests, verified SSH-signed commits and linear history; pull requests are squash merged. Delete the branch after the merge and start the next one from `origin/main`.
- Tags on `main` only; the workspace version in `Cargo.toml` tracks the latest tag. Cutting a release is step 8 of the maintainer checklist in `docs/USAGE.md`; item 11 there is what the 0.5.0 tag taught (LFS, dist, moving a tag, the PyPI run, npm naming). Read both before touching `release.yml`, `pypi.yml` or the dist config; after editing the dist config run `dist generate`, never hand-edit `release.yml`.
- `.github/workflows/ci.yml` runs on every push to `main` and every pull request: fmt, Clippy with the deny lints (the full CLI and the engine-only `--no-default-features` CLI), build and test on Ubuntu (AVX2) and macOS (NEON), the benches compiled, the mutation-fuzz harnesses at a higher count, the 639-line guard, forge-core's gate, cargo-deny on the three workspaces, cargo-semver-checks on `urna-format` against the pull request's base, a Windows job (Clippy, the CLI unit tests and `setup_e2e`: the only Windows check before a tag, so a crossterm or path change that breaks only there shows up there), ruff, and a bounded cargo-fuzz smoke on nightly. It is `release_check.sh` minus the LFS corpus measurement.
- On the nightly schedule (or `workflow_dispatch`) CI instead runs a 30-minute soak per fuzz target with the corpus cached between nights, and `urna-format`'s suite under Miri.
- A `v*` tag runs the release:
  - `tag-verify.yml` (the signature, then `scripts/release_preflight.py --tag`) is the first job of `build-wheels.yml` inside the release, and runs again before a PyPI upload: a lightweight or unsigned tag, a key missing from `.github/allowed_signers`, or a version that does not agree stops the release before the host job releases anything.
  - `release.yml` (cargo-dist): archives for 5 targets, checksums, Sigstore attestations, the Homebrew formula `urna`, the npm package `@urna/cli`, the embedder payload, and `publish-crates.yml` for urna-format, urna-runtime and urna.
  - The wheels (maturin abi3, 4 platforms) build in `build-wheels.yml`, a dist local-artifacts job, so the host and every publish job wait for them; the release carries them with a `.sha256` and an attestation each. PyPI is the `publish-pypi.yml` publish job: it dispatches the top-level `pypi.yml` on the tag (trusted publishing does not take a reusable workflow) and waits for that run, whose upload skips wheels already published with the same sha256.
  - `install-test.yml` runs inside the release run once it is announced and tests the installed product per platform and per channel (one-liner, Homebrew, npm, bun, pnpm, yarn, binstall, wheel), each ending in `urna setup --yes` and `urna doctor`.
- Git LFS tracks `*.urna`, `*.safetensors` and datasets, including `data/corpus_next.v1.urna` and the potion table; the golden fixtures stay in regular git. No release job touches LFS (`scripts/fetch_potion.sh` fetches the table from its pinned upstream and checks it against the pointer).
- `data/demo/` is gitignored; `data/demo/Instructions.md` names what it holds and where the pt-BR corpus is rebuilt (the fakenews-ptbr-urna-benchmark repo, not this one). Only `measure_presets.py` and `release_check.sh` need the baseline corpus, and they read the LFS file, never the datasets. `data/measure/corpus_*.urna` are regeneration artifacts and gitignored; the JSON baselines next to them are tracked.
- `scripts/pre-commit` aborts a commit that stages a data artifact off the allow-list (the PHI backstop). Install it per clone: `cp scripts/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit` (a copy, not `core.hooksPath`, so the LFS hooks keep working).
- Commit messages in plain English, no conventional-commits prefix; the body explains the why, the diff shows the what. The pull request title says what changes, the body why and how it was tested; on squash they become the commit on `main`.

# Docs and style

- No emoji. No em dash, the long dash character: use a comma, a semicolon, a period or a regular hyphen.
- Short paragraphs, direct voice, no marketing copy. Docs are task-oriented: what it does, how to run it, an example.
- Every doc starts with a YAML header: `project`, `audience`, `status`, `last-updated`, `domain` (skills also carry `name` and `description`). Exempt: `README.md` (packaged by crates.io, PyPI and npm; GitHub renders front matter as a table), `docs/LICENSE`, `.github/pull_request_template.md` (its text becomes the pull request body), `llms.txt` (it follows the llms.txt format) and the demo corpus documents under `python/forge/demo_corpus/` (they are data).
- `llms.txt` is discovery for LLMs and search: title, summary, links with a line each. It points at the docs and carries no instruction; this file is the instruction.
- `docs/arc/ARC.toml` is the single architecture reference: narrative (system_view, contract, quality, risks), file inventory and the Mermaid map of the build and query flows (`diagram.source`). After any change to a module, boundary, flow, public contract, storage or runtime behavior, update it in the same change: bump `last-updated`, append a dated note to `summary`, add new files to the inventory. No second architecture document.
- Docs are corrected in place. History and decisions, including a decision that turned out wrong and what replaced it, go to `docs/CHANGELOG`, the commit and the pull request; never as "changed x to y" notes inside a doc.
- Naming: directories, docs and assets in kebab-case English; source files idiomatic to their language. Propose a rename as `mv` commands, fix every import it touches, run the tests.
- `.editorconfig` is the base formatting: UTF-8, LF, 4-space indent (2 for TOML, YAML, JSON), final newline.

# File hygiene

Hard limit is 639 lines per code file. Human working memory holds 4 plus or minus 1 chunks at once (Cowan 2001, refining Miller), and a file that does not fit that window forces context switching, heavier diffs and more bugs.

A file created or modified that goes over 639 lines is read in full (what it does, what it depends on, who imports it) and split by responsibility into modules that each do one thing, with imports and the public surface kept and the tests passing with the same count. Exempt: tests, data and generated files, lockfiles, JSON, YAML, TOML, RON, JSONL, CSV, datasets and vendored files. `ci.yml` enforces the limit on `crates/**/src/**`; `release_check.sh` counts every non-test `.rs` under `crates/`, benches and examples included.

# Finishing a task

Run `.contracts/.agents/.skills/AFTERWORK.md`: it names which file owns what and where a lesson goes. On the way, sweep the session's changes for dead code, temporary scripts, stray files and files outside the folder their role belongs to; delete or move them, fix what they touched, run the tests. Write a temporary task manifest under your tmp folder, never in the tree.

# Gotchas

- Rebuild `python/_urna.so` after every Rust change to urna-format, urna-runtime or urna-python. The Python tests `dlopen` it, so a stale `.so` passes tests against old code. `release_check.sh` rebuilds it; by hand you must remember.
- `crates/urna-cli` has its own MSRV, 1.88 (Ratatui 0.30's floor); urna-format, urna-runtime and urna-python keep the workspace 1.85. With 1.88 Clippy suggests let-chains in the CLI, which is why its nested `if let`s are collapsed.
- NEON f16: `float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94, above the workspace MSRV. `crates/urna-runtime/build.rs` probes the compiler and emits `cfg(neon_f16)` at 1.94 and up; that cfg gates `simd/neon.rs::dot_f32_f16_neon`, older toolchains take the scalar f16 kernel, and the kernel carries `#[clippy::msrv = "1.94"]`. Keep build.rs and the cfg unless the workspace `rust-version` reaches 1.94.
- `urna-format` runs under Miri on the nightly schedule. A new test there that calls zstd (C code Miri cannot run), converts f16 through `half` on aarch64 (inline asm) or builds an fsst table (too slow interpreted) carries `#[cfg_attr(miri, ignore)]` with the reason, like the existing ones; the mutation and property harnesses stay native.
- The terminal UI owns stdout and stderr while a screen is up: no `println!` or `eprintln!` from code the UI calls (`pyenv::set_quiet(true)` silences the interpreter note; workers report through channels). Every frame ends with `pal::fit`, the 256-color and `NO_COLOR` fold. To see a screen for real: `tmux new-session -d -x 112 -y 28`, `tmux capture-pane -p -e -N`, render the escapes (SGR state carries across lines; strip OSC 8 before measuring columns).
- hyperrat links go through `hud::link`: hyperrat puts the whole OSC 8 sequence in one cell and Ratatui's diff then skips that many cells; `hud::link` forces the diff width to the label's. Never render `hyperrat::Link` directly.
- `ask` and `retrieve` embed offline, routed by the manifest model:
  - A potion corpus uses `python/forge/embed_query_potion.py` (numpy + tokenizers, no torch, no socket).
  - A corpus whose default text space is a registry model (wemm, CLIP, SigLIP2, Jina) goes through `python/forge/embed_query_model.py`, which loads it locally; network only with `URNA_ALLOW_DOWNLOAD=1`. An open_clip preset with a `revision` (SigLIP2) loads weights and tokenizer from `snapshots/<revision>` of the HF cache, never `refs/main` nor the hub name: by name, transformers' AutoTokenizer fails offline on a repo without `config.json` even fully cached. Its `model_hash` must not move, so the checkpoint goes into the built-in architecture with the tag's preprocess (`local-dir:` builds the preprocess from JSON and its repr differs); a missing snapshot file is `SnapshotMissing`, exit 3 with `urna-fetch:`. These pins (`hf_repo`, `revision`, `weights_file`, `snapshot_files`) are separate from `install` (`InstallSpec`): the installer verifies a file fingerprint, and an open_clip `model_hash` hashes the loaded tensors, so SigLIP2 stays out of the catalog with that reason.
  - The pt-BR demo corpus's model is the registry preset `minilm-multilingual` (kind `st_text`): its adapter calls `python/embed_query.py`'s own `load`/`encode`/`fingerprint`, so corpora built before the preset keep their `model_hash` (`tests/test_query_embedder_routing.py` case 10 checks the benchmark corpus). Any other sentence-transformers model no preset names takes the same `embed_query.py` path. The model has to be in the local HF cache: offline by default, `URNA_ALLOW_DOWNLOAD=1` fetches it once. torch loads on every query, so an ask on that path takes seconds, not milliseconds.
  - A failed embed is a typed `cmd/embed_failure.rs` value: script missing, payload incomplete (a script inside an installed payload that lacks a file of `cmd/payload.rs` REQUIRED, or an import error naming a payload module: never a pip hint), packages missing (the embedder prints `urna-needs: <pip spec>...`), weights missing (`urna-fetch: <model>`), model incompatible (no backend, a refused preset, a name or dim mismatch) or a model_hash mismatch. The CLI prints the long form with the fix, the explorer's ask tab the one-line `short`. A new failure mode gets a variant there, not a string match in the UI.
  - The interpreter is `pyenv::resolve_interpreter`, one ladder for every Python child (usage section 11 is the user-facing copy): `URNA_PYTHON`, then the venv `urna setup` made (`<data root>/urna/venv`), then the nearest `.venv/bin/python` walking up to four ancestors of the cwd, then `python3` on path. Every embedder script, `search-text`'s included, resolves through `embed_gate::installed_script_in`: the repo layout (`python/<rel>`, from the cwd and from a dev binary's own checkout), then `<root>/urna/<rel>` for each data root in turn (`URNA_DATA_DIR`, `XDG_DATA_HOME`, `~/.local/share`, `%LOCALAPPDATA%`, `<exe>/../share`). Without the forge deps the embed step fails with `ModuleNotFoundError`. The release payload carries every query embedder (`urna/embed_query.py` and `urna/model_fingerprint.py` beside `urna/forge/`, plus `urna/VERSION`, the release it came from; setup replaces a payload from another release, without the stamp or missing a file of `cmd::payload::REQUIRED`, as one restorable step with the stamp written last, never the venv); a registry model still needs its own deps (torch, sentence-transformers or open_clip) in that venv, which `embed_query_model.py` names, exit 4.
  - The model install is one operation, `tui/setup/models.rs`, shared by `urna setup --model` and the explorer's install panel (`tui/app/offer.rs`, opened by a `DepsMissing` or `WeightsMissing` query on a model the catalog offers). The catalog is `python/forge/catalog.json`, generated from the registry by `python/forge/model_catalog.py` and checked in: a preset is offered only with an `InstallSpec` (pinned revision, the exact files, their `model_hash`), and the stage script and `tests/test_model_catalog.py` refuse a stale copy; run `python python/forge/model_catalog.py --write` after changing a preset. The file list is part of the identity: the fingerprint hashes every relevant file present, so fetching a repo's `pytorch_model.bin` beside `model.safetensors` changes the hash. Packages go only into the venv setup manages (setup: its own data dir's; the explorer: the one the query ladder runs), never into an `URNA_PYTHON` pin. `forge/install_model.py` fetches through `hf_hub_download` with a progress class: the hub serves weights over Xet, which writes no partial file, so a watcher on the blob dir sees nothing until the end.
  - The flagship e2e tests (`cli_e2e.rs`, `python/forge/test_retrieve.py`) need those deps and skip without them; `release_check.sh` does not run them.
- The pt-BR fingerprint: the model fingerprint reads the local sentence-transformers cache. Populate it once, `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"`, or the fingerprint test fails.
- A conflict resolved by merging `main` into a pull-request branch (the GitHub button included) can keep one side of a definition and the other side's users: the #271 merge kept `main`'s `ModelPreset` without the four snapshot fields while the SigLIP2 preset still passed them, and `main` stopped importing the registry. Ruff does not see it; run `release_check.sh` (or at least `python python/forge/model_catalog.py --check`, which CI's python job runs) on the merged tree before the squash.
- Squash merge replaces the branch history: the pull request lands on `main` as one commit with a new hash, so a branch that keeps living after its merge conflicts on every file the squash touched. Delete merged branches; never rebase old work onto a merged branch. A stacked pull request keeps its base's pre-squash commits: after the base merges, merge `origin/main` into it locally, check that `git diff origin/main...HEAD` is only its own change, then merge; afterwards `git diff <tested commit> origin/main` must be empty.
- `cargo clean` costs 30 to 60 s of rebuild; incremental compilation handles most edits.
- Case-only renames on macOS: the filesystem ignores case and git runs with `core.ignorecase=true`, so renaming `usage.md` to `USAGE.md` on disk does not register. Use `git mv -f old New`.
- Repo-wide replaces go through `git grep -l`, never `grep -r`: gitignored third-party clones live under `tools/` and `TMP/`, and a recursive grep edits them too. Run package managers (npm, bun, pnpm) from a temporary directory, not from the repo root, or they leave a `package.json` behind.
- A dev-built binary always finds its own checkout (`embed_gate::exe_repo_root`), so a test of what an installed binary resolves from the data roots copies the binary out of `target/` first and points `HOME`, `XDG_DATA_HOME` and `URNA_DATA_DIR` into its scratch dir: `paths::data_roots` includes `~/.local/share`, so otherwise it finds the machine's real payload and venv (`crates/urna-cli/tests/embedder_resolution.rs`).
- macOS kills an overwritten binary: copying a fresh `target/*/urna` over an existing one makes every later run exit 137 (the code signature no longer matches). `rm -f` the target before `cp`.
- `release_check.sh` hides Clippy's output: when it stops at Clippy, run `cargo clippy --workspace --all-targets -- -D warnings` to see the lint. It also hides the names of failing tests (`passed=N failed=M` only): rerun `cargo test --release --workspace` by hand to see them.
- A unit test never reads the process environment through the code it tests: `URNA_PYTHON`, `URNA_DATA_DIR` and friends leak in from the shell that runs the gate (`URNA_PYTHON=.venv/bin/python ./scripts/release_check.sh` is the documented way). The probe (`Scan::probe`, `resolve_interpreter`) reads the env once and hands a value down; the pure function under test takes that value.
- Verifying a signed tag or commit locally: `git -c gpg.ssh.allowedSignersFile=.github/allowed_signers verify-tag vX.Y.Z`. Plain `git tag -v` fails without the setting, and `%G?` prints `N` even for a signed commit.

# Known gaps

Documented limitations, not bugs to fix in passing. Flag them in any work that touches these areas.

- `search-text` boots a Python process per call: fork, import sentence-transformers and torch, load the model, embed, exit. About 4 s warm and 7 s cold on an M-series Mac (sentence-transformers 6.1, torch 2.14, the MiniLM); `ask` and `retrieve` on a sentence-transformers corpus pay the same. The latency tables measure the search path after the vector is ready, not end to end; Python-driven workloads (`UrnaFile.search` in a loop) avoid it.
- The BM25 tokenizer is word-segmented only (`crates/urna-runtime/src/bm25/tokenize.rs`, non-alphanumeric Unicode boundaries): right for Latin, Cyrillic, Greek, Devanagari; wrong for CJK, Thai, Lao, where an unspaced run of characters is one token, so only an identical run matches and recall drops. Disable BM25 there (`with_bm25=False`) until a language-aware tokenizer ships.
- Package channels ship the bare binary; `urna setup` is a step, not a hook. Homebrew, npm, cargo install and binstall lay down the binary alone, `urna doctor` fails until setup runs (the first failing check names the code: 2 with no interpreter, 3 with one that lacks numpy and tokenizers, 4 with the deps there and no embedder payload), and no channel runs it for the user (npm hides postinstall output, dist has no formula hook). Setup needs `curl` on path and uv or a python3 with `venv`.
- comfy-tabs and comfy-toaster are not dependencies on purpose: both are source-available under SA-PS:DA (commercial use needs a license), incompatible with an MIT product. The tab pills are drawn in `app/chrome.rs`; the toasts adapt the MIT/Unlicense `ratatui-toaster`.
- The ST registry models have measured cost cliffs: wemm-2b runs fp16 on MPS with `image_max_side=768` (about 0.6 img/s); jina-v5-omni-nano has no `image_max_side` default and embeds at native resolution (about 0.3 img/s); changing either invalidates that model's cache by design (the knob is recipe-hashed).
- The semantic default embedder is English: `potion-base-8M` is distilled from `bge-base-en-v1.5`. English synonyms cluster tightly (car ~ automobile +0.78, car ~ banana +0.04); non-English text rides English subword rows and the signal is weak (carro ~ automovel +0.08, carro ~ banana -0.05). A primarily non-English corpus needs a multilingual sentence-transformers model or a multilingual potion table; the lexical floor is language-agnostic but literal.

# Documentation

- `README.md`: the storefront: what it is, install, the two screens, quickstart, Python, CLI, benchmarks.
- `docs/USAGE.md`: how-to for every verb, `urna setup` and `urna tui`, presets, offline mode, citations, the model registry and multi-model spaces (section 12), declarative builds (13), the compression levers and the dual quality gate (14), and the reference section: the table of every `URNA_*` environment variable, every install channel, verification, offline notes, the maintainer checklist.
- `docs/arc/ARC.toml`: the architecture reference described above.
- `docs/CHANGELOG`: every release and the unreleased deltas, with the why and the measured numbers.
- `docs/BENCH.md`: Urna against usearch, hnswlib, sqlite-vec and LanceDB; regenerated by `python/tools/bench_competitors.py`, never edited by hand.
- `docs/SECURITY.md`: reporting, supported versions, scope, hardening, the data-governance posture.
- `docs/CONTRIBUTING.md`, `docs/CODE_OF_CONDUCT.md`, `docs/LICENSE` (MIT).
- `data/demo/Instructions.md`: the frozen pt-BR gate corpus and its hashes, the image sources.
- `scripts/release_check.sh`: read it; it documents the gate by being the gate.
- `fuzz/README.md`: the four cargo-fuzz targets, running a soak, regenerating the seeds, turning a finding into a test.
- `.contracts/.agents/.skills/AFTERWORK.md`: the end-of-task walk; `.github/pull_request_template.md` carries it as checkboxes.
- `llms.txt`: discovery for LLMs and search.
