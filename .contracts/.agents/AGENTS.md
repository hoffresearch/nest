---
project: urna
audience: ai coding agents and human contributors
status: active
last-updated: 2026-09-27
domain: repo-ops
---

# agents

the one instruction source for this repo. the root `CLAUDE.md` is a symlink to this file; never edit the link or add a parallel instruction file (GEMINI.md, CODEX.md, cursor rules). most agent tooling reads `.contracts/.agents/AGENTS.md` on its own; point the rest here on init.

the principal author writes fast and uses voice transcription: typos, caps lock and missing accents are common. read the intent; do not flag tone or project emotional risk.

# start here

1. read `docs/arc/ARC.toml` in a short pass: the architecture, the file inventory, the build and query flows.
2. work on a short-lived branch off `origin/main`, one pull request per topic, squash merged.
3. every change ships with real tests: happy path, error path, one edge case, against real artifacts (built `.urna` files, golden fixtures, real corpora), no mocks. nothing merges without executable proof.
4. before the pull request: `./scripts/release_check.sh` (the gate) and `.contracts/.agents/.skills/AFTERWORK.md` (every doc the change owns, updated in place).
5. the hard rules below protect three things: the frozen file format, the offline promise (no network stack in the binary, no socket at query time) and the release channels. everything else is judgment. when a rule stands in the way of a better design, say so in the pull request and change the rule together with the change.

# autonomy

on your own: branches, commits, pull requests, the test suites, workflows that publish nothing (ci, an `install-test` dispatch), installs into temporary prefixes, the docs a change owns.

ask first, because it publishes or is hard to undo:

- pushing or moving a tag. a `v*` tag publishes to crates.io, npm, homebrew and pypi, and a crates.io version is permanent.
- merging past a blocked review (`--admin`): never. wait for the maintainer.
- force-push: `main` never (the ruleset blocks it); a feature branch only with `--force-with-lease` and an explicit ok.
- `git add -A`, `--no-verify`, deleting a remote branch, changing repository or organization settings, rotating a secret.

secrets live in the github repository secrets (`CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `HOMEBREW_TAP_TOKEN`) and the `pypi` environment. a token never goes into the tree, a commit, a log, a pull request or a chat message; one that did is compromised and gets revoked. the trusted-publisher path (oidc) needs no token at all: prefer it where a registry offers it.

# build and test

- `cargo build --workspace`, `cargo build --release --workspace`
- `cargo test --workspace`: every rust test (unit, integration, golden); the count it prints is the one `docs/CHANGELOG` cites
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (warnings are errors)
- `cargo clippy -p urna --no-default-features --all-targets -- -D warnings`: the engine-only cli, without the terminal ui, has to stay clean too; ci runs both
- `cargo deny check`: advisories, licenses (the allowlist in `deny.toml`), bans and sources over the lockfile. a yanked crate, a copyleft-only license or a git dependency fails ci; the same policy file covers the other two workspaces with `--manifest-path forge-core/Cargo.toml` and `--manifest-path fuzz/Cargo.toml`
- `cargo semver-checks -p urna-format --baseline-rev origin/main`: the rust api of the frozen format against the pull request's base; ci fails a pull request that breaks it (in 0.x a minor bump is the major bump)
- `cargo bench -p urna-runtime --no-run`: the criterion benches (simd, rerank, hnsw_build) have to compile; ci checks that, the numbers are not a gate
- `sh scripts/ruff_check.sh`: ruff over the one python file list shared with ci (`URNA_PYTHON=.venv/bin/python` picks the interpreter)
- `./scripts/release_check.sh`: the full pipeline (the rust suite in release, the extension rebuilt, nine python suites, ruff when importable) plus the regression gates against `data/measure/baseline.json`; exits non-zero on any failure. it is the definition of pull-request ready
- `forge-core/` is a separate cargo workspace outside `crates/` (the ingestion layer, the frozen `.fci` schema). `--workspace` and `release_check.sh` never reach it; run `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` with `--manifest-path forge-core/Cargo.toml`
- `fuzz/` is the third cargo workspace (cargo-fuzz, nightly toolchain): `sh scripts/fuzz_soak.sh [seconds]` runs every target with the corpus kept under `fuzz/corpus/`; `fuzz/README.md` has the targets and how a finding becomes a test

single targets:

- `cargo test -p urna-format`, `cargo test -p urna-runtime`
- `cargo test --release -p urna-runtime --test hnsw_recall`: the recall regression; debug is 30x slower and hits the cargo test timeout
- `cargo test -p urna`: the cli integration tests (cargo builds the binary they run, `CARGO_BIN_EXE_urna`)
- `cargo run -p urna-format --example regen_golden`: regenerate the byte-frozen golden fixture, only when the format really changed

## python

the extension is built by hand in dev:

```
cargo build --release -p urna-python --features pyo3/extension-module
cp target/release/lib_urna.dylib python/_urna.so   # macos
cp target/release/lib_urna.so   python/_urna.so    # linux
```

`pyo3/extension-module` keeps libpython out of the cdylib; without it the `.so` hard-links a libpython path and segfaults under statically embedded interpreters (uv's python-build-standalone). `release_check.sh` builds with the feature and pins `PYO3_PYTHON` to the test interpreter.

the published wheel is maturin, staged; never edit `packaging/staging/` by hand:

```
python scripts/stage_wheel.py
(cd packaging/staging && maturin build --release)   # wheel lands in target/wheels/
```

`packaging/pyproject.toml` is the one source of the wheel project. the staging script copies it plus `python/urna.py` (as `urna/__init__.py`), `python/urna_cli.py` (as `urna/_cli.py`), `python/forge/embed_potion.py` and the potion table. abi3, python 3.12 and up.

the wheel installs its own console script named `urna` (`python/urna_cli.py`): a read-only shim over the library api (`validate`, `inspect`, `stats`, `search`) so `uvx --from urna urna ...` works. it is not the rust binary; `ask`, `retrieve`, `build`, `doctor` and the terminal ui exist only there. `pip install "urna[embed]"` adds numpy and tokenizers for `urna.embed_potion`; the core surface needs nothing beyond the wheel.

the python tests are plain scripts with `if __name__ == "__main__"`; `pytest tests/` does not work. they need the built `.so` first:

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
```

`release_check.sh` runs nine of them; `test_offline_guard.py`, `test_blob_bridge.py` and `test_space_bridge.py` run by hand.

`test_image_corpus.py` covers the forge image pillar (encode and decode, gop probe, sharding, ordering) with a stub embedder and skips cleanly without ffmpeg's av1 and avif encoders. building a real image corpus (`python/forge/embed_image.py`) needs `open_clip` and torch, outside the default forge dependency group.

the forge build-side default embedder is the vendored model2vec/potion-base-8M static table (`python/forge/embed_potion.py`): offline, no torch, no network.

- its self-test is `python python/forge/test_embed_potion.py`. it needs numpy and tokenizers (`uv pip install numpy tokenizers`, the `forge` dependency group) and the table, which is git-lfs: `git lfs pull`, or `sh scripts/fetch_potion.sh` when lfs is unavailable.
- the self-test proves the semantic jump (car ~ automobile far above car ~ banana), determinism, f32 stability and that no socket opens at embed time. `python python/forge/recall_harness.py` shows per-query recall against the floor.
- the lexical bag-of-words floor (`python/forge/embed_default.py`) is stdlib-only, with its own self-test: `python python/forge/test_embed_default.py`.
- both fingerprint to a `model_hash` recorded in provenance; neither runs in `release_check.sh`.
- two more forge self-tests, also outside `release_check.sh`: `python python/forge/test_model_registry.py` (the registry contract, no ml deps) and `python python/forge/test_embed_st.py` (the sentence-transformers worker against a local wemm-2b snapshot; skips without it).

python entry point: `sys.path.insert(0, "python"); import urna`. the loader finds `_urna.so` or `lib_urna.dylib`.

# layout

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

the full map (every file, the flows, the contracts) is `docs/arc/ARC.toml`. key rust deps: memmap2, rayon, zstd, half, bytemuck, sha2, thiserror, clap, serde.

three cargo workspaces, one policy each: the root `Cargo.toml` (`crates/*`, the one that ships), `forge-core/` and `fuzz/`. `deny.toml` is the cargo-deny policy for all three. `clippy.toml` pins cognitive complexity at 15 (clippy's default is 25) and 7 arguments per function; a legitimate exception gets `#[allow(clippy::...)]` at the site, the threshold never moves. `rustfmt.toml` pins the stable defaults at width 100.

the cli has three groups, which `urna --help` tags and orders:

- engine verbs, file and vector in, never run python: `inspect`, `validate`, `stats`, `media`, `search`, `search-ann`, `search-graph`, `search-space`, `search-text`, `benchmark`, `cite`, `doctor`.
- agent verbs over the same engine, `cmd/agent/`: `build` (a declarative corpus build, launching `python/tools/urna_forge.py`), `ask` (text in, cited answer out, `--disclose answer|explain`), `retrieve` (json or jsonl of cited spans; `score` is the exact rerank value). they embed offline and route the query embedder by the manifest model: potion corpora keep the potion script, registry models go through `python/forge/embed_query_model.py`. the search path is routed by what the file carries (`MmapUrnaFile::search_routed`, shared with `search-text`): hybrid when a bm25 section is present, hnsw when an hnsw section is present, exact otherwise; the manifest `index_type` names the vector index only, so a `hybrid` preset file (`index_type = "hnsw"`, `supports_bm25 = true`) takes the hybrid route. the graph is reached only through `search-graph`. the build contract is `docs/USAGE.md` sections 12 to 14.
- the terminal ui, `src/tui`:
  - `urna setup` is the installer every channel ends in: scan the machine, show the plan, install, verify. the payload comes through a system `curl` child with the sha256 checked while it streams; `--yes` for scripts. exit codes: 10 download, 11 checksum, 12 unpack, 13 python env, 14 blocked, above doctor's 2 to 6.
  - `urna tui [file]` is the explorer: home, corpus, ask, health, a file picker, a hand-off to setup. a bare `urna` on a terminal opens it; in a pipe it prints help and exits 2.
  - `URNA_RELEASE_BASE=file:///dir` points setup at a local release (the e2e tests do this).

# contract

the format and runtime invariants. a change that touches them needs the tests named next to each.

- rust edition 2024, resolver 3, `thiserror` errors, never a panic in library code. `repr(C)` plus `bytemuck::Pod` for the binary layout, integers little-endian unsigned.
- every `unsafe` block carries a `// SAFETY:` comment naming the invariant; clippy denies undocumented ones and `unwrap` outside tests (`[workspace.lints]`). prefer `bytemuck` casts over raw parts; keep raw-pointer kernels behind a safe dispatcher that asserts every length in release.
- reading and bounds: fixed-width fields through `urna_format::bytes::{le_u32, le_u64, le_f32, array32}` (typed `UnexpectedEof`, never `try_into().unwrap()`); header-derived sizes with checked arithmetic; cursor checks as `need > remaining`, never `pos + need > len`; f32 scores sorted through the runtime's `order` module (`crates/urna-runtime/src/order.rs`, nan-last total order), never `partial_cmp(..).unwrap_or(Equal)`.
- binary format v1 is frozen. encodings 4 to 255 and section ids 0x09 and up are reserved within v1 and additive; `URNA_FORMAT_VERSION` moves only when an existing field changes meaning. the section-id map is the `contract` array of `ARC.toml`. shipped so far: encodings 1/2/3 (zstd, float16, int8) and 7 (int4); sections 0x07 hnsw, 0x08 bm25, 0x0C graph, 0x14 blob_refs, 0x16 blob_span_overlay, 0x15 space_table with the 0x20 to 0x2F embedding band; all content_hash-excluded.
- a decoder change runs the mutation harness before the pull request: `cargo test -p urna-format --test mutation_fuzz -p urna-runtime --test mutation_fuzz` (`URNA_MUTATION_ITERS=25000` for a soak), then `sh scripts/fuzz_soak.sh` (nightly, cargo-fuzz). a new codec gets an arm in `fuzz/fuzz_targets/section_decoders.rs`; a finding becomes a `tests/negative_*.rs` before the fix and a `fuzz/seeds/regress-*.bin`.
- hashes are `sha256:<64 lowercase hex>`: `header_checksum`, per-section `checksum` (physical bytes), `file_hash` (whole file), `content_hash` (decoded canonical sections, stable across encodings). same chunks, same model fingerprint and `reproducible=True` give byte-identical files, so `urna://content_hash/chunk_id` points at content, not at a copy.
- `UrnaFileBuilder` is a consuming builder (`add_chunk(self) -> Self`). `preset=` takes `exact` (raw + f32), `compressed` (zstd + f16), `tiny` (zstd + int8 + hnsw), `nano` (zstd + int4 block 64 + hnsw) or `hybrid` (zstd + f32 + hnsw + bm25). `micro` in the tables is not a preset value: it is the published name for `tiny` at `mrl_dim=256`, built with `urna.build(text_encoding="zstd", dtype="int8", mrl_dim=256, with_hnsw=True)`.
- matryoshka truncation is a build-time kwarg (`mrl_dim`): the python builder slices each l2-normalized row to its first K components and re-normalizes before quantization; the header `embedding_dim` becomes K and `full_dim` records the source. no runtime kernel change.
  - int4 needs the effective dim divisible by 64, so its ladder is 256, 192, 128. content_hash covers the truncated vectors.
  - the shipped MiniLM corpus is not mrl-trained, so truncation costs measured recall (`measure_presets.py --variants mrl<DIM>-<dtype>`).
- hnsw builds are deterministic given a seed; the bm25 index is sorted by term.
- `model_hash` fingerprints `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`. a zero placeholder is rejected at write time; a runtime model that differs from the corpus model fails with a typed error.
- simd dispatch: avx2 on x86_64, neon on aarch64, scalar fallback, f32 accumulators. `URNA_FORCE_SCALAR=1` forces scalar.
- the golden fixture `crates/urna-format/tests/fixtures/golden_v1_minimal.urna` is byte-frozen at 1366 bytes.
- the reader also accepts the `NEST` magic of files written by 0.4.0, before the rename (`tests/legacy_magic.rs`, fixture `legacy_v040_minimal.nest`, same layout and hashes); the writer never emits it, and any other magic is rejected.
- cli `search` takes a json f32 array; `search-text` shells out to `python/embed_query.py` and checks its `model_hash` against the manifest.
- python api: `urna.open(path)` returns a `UrnaFile` with `search`, `search_ann`, `search_hybrid`, `search_graph`, `search_space`, `retrieve`, `validate`, `inspect`, plus `chunk_ids()` (identity in file order, the key to match hits under any media ordering), `blob_refs()`, `has_blobs` and `blob_bytes(i)` (one inlined blob sliced off the mmap); hits carry `citation_id`, `source_uri`, offsets and the exact-rerank `score`.
- `cite` is tier-1 only: the stored canonical text plus the verifying hashes, never an original-byte reopen. `ask` and `retrieve` print the same text. no help text or doc claims otherwise.
- the binary links no network stack and the runtime never opens a socket. setup downloads through the system `curl`.

# workflow

- remote `git@github.com:hoffresearch/urna.git`, owner hoff research, maintainer brenner cruvinel (`brenner@hoffresearch.com`).
- `main` is the only long-lived branch. the ruleset requires pull requests, verified ssh-signed commits and linear history; pull requests are squash merged. delete the branch after the merge and start the next one from `origin/main`.
- tags on `main` only; the workspace version in `Cargo.toml` tracks the latest tag. cutting a release is step 8 of the maintainer checklist in `docs/USAGE.md`; item 11 there is what the 0.5.0 tag taught (lfs, dist, moving a tag, the pypi run, npm naming). read both before touching `release.yml`, `pypi.yml` or the dist config; after editing the dist config run `dist generate`, never hand-edit `release.yml`.
- `.github/workflows/ci.yml` runs on every push to `main` and every pull request: fmt, clippy with the deny lints (the full cli and the engine-only `--no-default-features` cli), build and test on ubuntu (avx2) and macos (neon), the benches compiled, the mutation-fuzz harnesses at a higher count, the 639-line guard, forge-core's gate, cargo-deny on the three workspaces, cargo-semver-checks on `urna-format` against the pull request's base, a windows job (clippy, the cli unit tests and `setup_e2e`: the only windows check before a tag, so a crossterm or path change that breaks only there shows up there), ruff, and a bounded cargo-fuzz smoke on nightly. it is `release_check.sh` minus the lfs corpus measurement.
- on the nightly schedule (or `workflow_dispatch`) ci instead runs a 30-minute soak per fuzz target with the corpus cached between nights, and `urna-format`'s suite under miri.
- a `v*` tag runs the release:
  - `tag-verify.yml` goes first, in dist's plan phase and again before the wheels build: a lightweight or unsigned tag, or a key missing from `.github/allowed_signers`, stops both runs before anything is built or uploaded.
  - `release.yml` (cargo-dist): archives for 5 targets, checksums, sigstore attestations, the homebrew formula `urna`, the npm package `@urna/cli`, the embedder payload, and `publish-crates.yml` for urna-format, urna-runtime and urna.
  - `pypi.yml`: maturin abi3 wheels for 4 platforms, on its own run.
  - `install-test.yml` runs inside the release run once it is announced and tests the installed product per platform and per channel (one-liner, homebrew, npm, bun, pnpm, yarn, binstall, wheel), each ending in `urna setup --yes` and `urna doctor`.
- git lfs tracks `*.urna`, `*.safetensors` and datasets, including `data/corpus_next.v1.urna` and the potion table; the golden fixtures stay in regular git. no release job touches lfs (`scripts/fetch_potion.sh` fetches the table from its pinned upstream and checks it against the pointer).
- `data/demo/` datasets are gitignored and fetched from the upstream sources in `data/demo/Instructions.md`; only `measure_presets.py` and `release_check.sh` need the baseline corpus. `data/measure/corpus_*.urna` are regeneration artifacts and gitignored; the json baselines next to them are tracked.
- `scripts/pre-commit` aborts a commit that stages a data artifact off the allow-list (the phi backstop). install it per clone: `cp scripts/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit` (a copy, not `core.hooksPath`, so the lfs hooks keep working).
- commit messages in plain english, no conventional-commits prefix; the body explains the why, the diff shows the what. the pull request title says what changes, the body why and how it was tested; on squash they become the commit on `main`.

# docs and style

- all lowercase, including headers (acronyms like `CLI` are the exception). no emoji. no em dash, the long dash character: use a comma, a semicolon, a period or a regular hyphen.
- short paragraphs, direct voice, no marketing copy. docs are task-oriented: what it does, how to run it, an example.
- every doc starts with a yaml header: `project`, `audience`, `status`, `last-updated`, `domain` (skills also carry `name` and `description`). exempt: `README.md` (packaged by crates.io, pypi and npm; github renders front matter as a table), `docs/LICENSE`, `.github/pull_request_template.md` (its text becomes the pull request body), `llms.txt` (it follows the llms.txt format) and the demo corpus documents under `python/forge/demo_corpus/` (they are data).
- `llms.txt` is discovery for llms and search: title, summary, links with a line each. it points at the docs and carries no instruction; this file is the instruction.
- `docs/arc/ARC.toml` is the single architecture reference: narrative (system_view, contract, quality, risks), file inventory and the mermaid map of the build and query flows (`diagram.source`). after any change to a module, boundary, flow, public contract, storage or runtime behavior, update it in the same change: bump `last-updated`, append a dated note to `summary`, add new files to the inventory. no second architecture document.
- docs are corrected in place. history and decisions, including a decision that turned out wrong and what replaced it, go to `docs/CHANGELOG`, the commit and the pull request; never as "changed x to y" notes inside a doc.
- naming: directories, docs and assets in kebab-case english; source files idiomatic to their language. propose a rename as `mv` commands, fix every import it touches, run the tests.
- `.editorconfig` is the base formatting: utf-8, lf, 4-space indent (2 for toml, yaml, json), final newline.

# file hygiene

hard limit is 639 lines per code file. human working memory holds 4 plus or minus 1 chunks at once (cowan 2001, refining miller), and a file that does not fit that window forces context switching, heavier diffs and more bugs.

a file created or modified that goes over 639 lines is read in full (what it does, what it depends on, who imports it) and split by responsibility into modules that each do one thing, with imports and the public surface kept and the tests passing with the same count. exempt: tests, data and generated files, lockfiles, json, yaml, toml, ron, jsonl, csv, datasets and vendored files. `ci.yml` enforces the limit on `crates/**/src/**`; `release_check.sh` counts every non-test `.rs` under `crates/`, benches and examples included.

# finishing a task

run `.contracts/.agents/.skills/AFTERWORK.md`: it names which file owns what and where a lesson goes. on the way, sweep the session's changes for dead code, temporary scripts, stray files and files outside the folder their role belongs to; delete or move them, fix what they touched, run the tests. write a temporary task manifest under your tmp folder, never in the tree.

# gotchas

- rebuild `python/_urna.so` after every rust change to urna-format, urna-runtime or urna-python. the python tests `dlopen` it, so a stale `.so` passes tests against old code. `release_check.sh` rebuilds it; by hand you must remember.
- `crates/urna-cli` has its own msrv, 1.88 (ratatui 0.30's floor); urna-format, urna-runtime and urna-python keep the workspace 1.85. with 1.88 clippy suggests let-chains in the cli, which is why its nested `if let`s are collapsed.
- neon f16: `float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94, above the workspace msrv. `crates/urna-runtime/build.rs` probes the compiler and emits `cfg(neon_f16)` at 1.94 and up; that cfg gates `simd/neon.rs::dot_f32_f16_neon`, older toolchains take the scalar f16 kernel, and the kernel carries `#[clippy::msrv = "1.94"]`. keep build.rs and the cfg unless the workspace `rust-version` reaches 1.94.
- `urna-format` runs under miri on the nightly schedule. a new test there that calls zstd (c code miri cannot run), converts f16 through `half` on aarch64 (inline asm) or builds an fsst table (too slow interpreted) carries `#[cfg_attr(miri, ignore)]` with the reason, like the existing ones; the mutation and property harnesses stay native.
- the terminal ui owns stdout and stderr while a screen is up: no `println!` or `eprintln!` from code the ui calls (`pyenv::set_quiet(true)` silences the interpreter note; workers report through channels). every frame ends with `pal::fit`, the 256-color and `NO_COLOR` fold. to see a screen for real: `tmux new-session -d -x 112 -y 28`, `tmux capture-pane -p -e -N`, render the escapes (sgr state carries across lines; strip osc 8 before measuring columns).
- hyperrat links go through `hud::link`: hyperrat puts the whole osc 8 sequence in one cell and ratatui's diff then skips that many cells; `hud::link` forces the diff width to the label's. never render `hyperrat::Link` directly.
- `ask` and `retrieve` embed offline, routed by the manifest model:
  - a potion corpus uses `python/forge/embed_query_potion.py` (numpy + tokenizers, no torch, no socket).
  - a corpus whose default text space is a registry model (wemm, clip, jina) goes through `python/forge/embed_query_model.py`, which loads it locally; network only with `URNA_ALLOW_DOWNLOAD=1`.
  - a corpus built with a sentence-transformers model outside the registry (the pt-br demo corpus, MiniLM) is not askable. `search-text` is its path, through `python/embed_query.py`, network on first use.
  - the embedder runs under `python3` unless `URNA_PYTHON` points at a venv with the forge deps; otherwise the embed step fails with `ModuleNotFoundError`. the release payload carries both query embedders; a registry model still needs its own deps (torch, sentence-transformers or open_clip) in that venv, which `embed_query_model.py` names, exit 4.
  - the flagship e2e tests (`cli_e2e.rs`, `python/forge/test_retrieve.py`) need those deps and skip without them; `release_check.sh` does not run them.
- the pt-br fingerprint: the model fingerprint reads the local sentence-transformers cache. populate it once, `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"`, or `urna_build_corpus.py` and the fingerprint test fail.
- squash merge replaces the branch history: the pull request lands on `main` as one commit with a new hash, so a branch that keeps living after its merge conflicts on every file the squash touched. delete merged branches; never rebase old work onto a merged branch.
- `cargo clean` costs 30 to 60 s of rebuild; incremental compilation handles most edits.
- case-only renames on macos: the filesystem ignores case and git runs with `core.ignorecase=true`, so renaming `usage.md` to `USAGE.md` on disk does not register. use `git mv -f old New`.
- repo-wide replaces go through `git grep -l`, never `grep -r`: gitignored third-party clones live under `tools/` and `TMP/`, and a recursive grep edits them too. run package managers (npm, bun, pnpm) from a temporary directory, not from the repo root, or they leave a `package.json` behind.
- macos kills an overwritten binary: copying a fresh `target/*/urna` over an existing one makes every later run exit 137 (the code signature no longer matches). `rm -f` the target before `cp`.
- `release_check.sh` hides clippy's output: when it stops at clippy, run `cargo clippy --workspace --all-targets -- -D warnings` to see the lint.
- verifying a signed tag or commit locally: `git -c gpg.ssh.allowedSignersFile=.github/allowed_signers verify-tag vX.Y.Z`. plain `git tag -v` fails without the setting, and `%G?` prints `N` even for a signed commit.

# known gaps

documented limitations, not bugs to fix in passing. flag them in any work that touches these areas.

- `search-text` boots a python process per call (300 to 500 ms: fork, import sentence-transformers, embed, exit). the latency tables measure the search path after the vector is ready, not end to end; python-driven workloads (`UrnaFile.search` in a loop) avoid it.
- the bm25 tokenizer is word-segmented only (`crates/urna-runtime/src/bm25/tokenize.rs`, non-alphanumeric unicode boundaries): right for latin, cyrillic, greek, devanagari; wrong for cjk, thai, lao, where an unspaced run of characters is one token, so only an identical run matches and recall drops. disable bm25 there (`with_bm25=False`) until a language-aware tokenizer ships.
- package channels ship the bare binary; `urna setup` is a step, not a hook. homebrew, npm, cargo install and binstall lay down the binary alone, `urna doctor` fails until setup runs (the first failing check names the code: 2 with no interpreter, 3 with one that lacks numpy and tokenizers, 4 with the deps there and no embedder payload), and no channel runs it for the user (npm hides postinstall output, dist has no formula hook). setup needs `curl` on path and uv or a python3 with `venv`.
- comfy-tabs and comfy-toaster are not dependencies on purpose: both are source-available under SA-PS:DA (commercial use needs a license), incompatible with an mit product. the tab pills are drawn in `app/chrome.rs`; the toasts adapt the mit/unlicense `ratatui-toaster`.
- the st registry models have measured cost cliffs: wemm-2b runs fp16 on mps with `image_max_side=768` (about 0.6 img/s); jina-v5-omni-nano has no `image_max_side` default and embeds at native resolution (about 0.3 img/s); changing either invalidates that model's cache by design (the knob is recipe-hashed). the siglip2 text tower resolves an hf tokenizer whose optional-file probes can fail in strict offline mode even with the snapshot cached (usage section 12 has the workaround).
- the semantic default embedder is english: `potion-base-8M` is distilled from `bge-base-en-v1.5`. english synonyms cluster tightly (car ~ automobile +0.78, car ~ banana +0.04); non-english text rides english subword rows and the signal is weak (carro ~ automovel +0.08, carro ~ banana -0.05). a primarily non-english corpus needs a multilingual sentence-transformers model or a multilingual potion table; the lexical floor is language-agnostic but literal.

# documentation

- `README.md`: the storefront: what it is, install, the two screens, quickstart, python, cli, benchmarks.
- `docs/USAGE.md`: how-to for every verb, `urna setup` and `urna tui`, presets, offline mode, citations, the model registry and multi-model spaces (section 12), declarative builds (13), the compression levers and the dual quality gate (14), and the reference section: the table of every `URNA_*` environment variable, every install channel, verification, offline notes, the maintainer checklist.
- `docs/arc/ARC.toml`: the architecture reference described above.
- `docs/CHANGELOG`: every release and the unreleased deltas, with the why and the measured numbers.
- `docs/BENCH.md`: urna against usearch, hnswlib, sqlite-vec and lancedb; regenerated by `python/tools/bench_competitors.py`, never edited by hand.
- `docs/SECURITY.md`: reporting, supported versions, scope, hardening, the data-governance posture.
- `docs/CONTRIBUTING.md`, `docs/CODE_OF_CONDUCT.md`, `docs/LICENSE` (mit).
- `data/demo/Instructions.md`: the upstream pt-br datasets and the corpus rebuild.
- `scripts/release_check.sh`: read it; it documents the gate by being the gate.
- `fuzz/README.md`: the four cargo-fuzz targets, running a soak, regenerating the seeds, turning a finding into a test.
- `.contracts/.agents/.skills/AFTERWORK.md`: the end-of-task walk; `.github/pull_request_template.md` carries it as checkboxes.
- `llms.txt`: discovery for llms and search.
