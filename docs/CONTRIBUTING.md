---
project: urna
audience: contributors
status: active
last-updated: 2026-10-06
domain: contributing
---

# Contributing

`urna` is maintained by [Hoff Research](https://hoffresearch.com). Author: Brenner Cruvinel ([brenner@hoffresearch.com](mailto:brenner@hoffresearch.com)). All contributions are welcome. Contributors and AI coding agents start from `.contracts/.ai/.agents/AGENTS.md`.

## How to contribute

1. Fork the repo at https://github.com/hoffresearch/urna.
2. Branch from `main`: `git checkout -b feature/short-description origin/main`.
3. Keep each PR focused on one concern. Small is better.
4. Validate the behavior the change affects. Reuse existing tests and add cases for new guarantees or regressions. Use real artifacts for file and installation behavior, and controlled service responses for failure and retry cases.
5. If the change alters architecture, module boundaries, data flow, or doc locations, update `docs/ARC.toml` in the same PR. Keep it concise and pragmatic. Do not add a separate human architecture doc; `ARC.toml` is the machine map, the human reference, and the mermaid diagram all in one file. Record an architecture decision, or a lesson that must stay as reference, as an ADR under `docs/ADR/` (layout and categories in `docs/ADR/README.md`).
6. Run checks appropriate to the change and the required CI checks. Use `./script/fullcheck.sh` for format, runtime, performance or broad integration changes; it rebuilds the extension, runs Rust and Python tests, lint and the corpus regression measurements. Focused script or documentation changes can use targeted checks, with their scope explained in the PR; editorial changes do not repeat corpus measurements. `.github/workflows/gatecheck.yml` covers the Rust side on Linux, macOS and Windows plus checks the local gate does not run (cargo-deny, cargo-semver-checks, the engine-only Clippy, the benches compiled, a cargo-fuzz smoke, the embedder payload staged under Python 3.10). Its Python job runs ruff, the model catalog check and the release suites that need no built extension; the suites that load `_urna.so` run only in the local gate, so run them locally. A pull request that touches a release input also runs the release rehearsal (`rehearsal.yml`), a required check.
7. Commit with a clear message in plain English. No conventional commits prefix.
8. Start from an issue: one change is one issue; several independent items are an epic with one sub-issue each. Give the issue and the PR the right label. Open a PR against `main` whose body starts with `Closes #<issue>` and lists its checks as checkboxes, then ask for a review. Only the maintainer merges, with a merge commit, so every commit of the pull request stays in `main`'s history. `main` is guarded by one ruleset (`opensource`): one approving review, verified (signed) commits, a branch up to date with `main`, resolved review conversations and a passing `rehearsal` check (the release rehearsal, dispensed when the change touches no release input); admins can bypass it. Sign your commits (`git config commit.gpgsign true` with an SSH or GPG key registered on GitHub). A pull request that builds on another opens from the updated `main` after that one merges.

## Setup

Requires Rust edition 2024 (`rustc >= 1.88`: the CLI crate's floor, set by Ratatui; the format, runtime and Python crates alone build on 1.85) and Python 3.12+.

```
git clone https://github.com/hoffresearch/urna.git
cd urna

cargo build --release --workspace
# the python extension is a separate build: `pyo3/extension-module` keeps
# libpython out of the cdylib (without it the .so segfaults under uv's
# standalone interpreters)
cargo build --release -p urna-bridge --features pyo3/extension-module
cp target/release/lib_urna.dylib python/_urna.so   # macOS
cp target/release/lib_urna.so   python/_urna.so    # linux

python3 -m venv .venv && source .venv/bin/activate
pip install ruff pyyaml numpy tokenizers pillow sentence-transformers pandas zstandard pyarrow
```

`numpy` and `tokenizers` are the forge deps the potion embedder needs; `pyyaml` is for the release rehearsal generator and its tests; `pillow` is for the image tests; `sentence-transformers` only for the pt-BR corpus and `search-text`.

`data/corpus_next.v1.urna` is tracked via Git LFS; it is the frozen baseline of the regression gate, and `data/demo/Instructions.md` gives its hashes. Demo data under `data/demo/` is local-only and gitignored. Without it, runtime unit tests still pass.

## Conventions and writing style

These conventions are not aesthetic preferences. They exist to keep the repo readable for humans, agents, and vector search at the same time. If you find yourself wanting to break one, open an issue first and explain why; do not silently deviate. The goal is gentle communal pressure to keep the codebase legible.

### Naming

- Directories and assets are **kebab-case English** (`data/`, `docs/`, `demos/`, `assets/image/`, dataset folders); Rust workspace conventions (`crates/`, `target/`) and language defaults (`python/`, `script/`, `tests/`) stay as their stacks expect.
- Names line up by length ([ADR-0002](ADR/STRUCTURE/0002-short-folder-names-published-package-names.md)): a crate folder is six letters (`crates/engine`, `crates/clitui`) and its published package carries the `urna-` prefix (`urna-engine`; the CLI is `urna`); a file in `script/`, a workflow and a test after `test_` are nine letters (`script/preflight.py`, `gatecheck.yml`, `tests/test_hashguard.py`); a top-level folder is one short word (`script/`, `demos/`, `packs/`). Names fixed by a tool stay (`release.yml`, `Cargo.toml`, `README.md`). A new file picks a name of the same length as its neighbours. The `renameops` agent skill (`.contracts/.ai/.agents/.skills/renameops/SKILL.md`) is the procedure for a rename or a move.
- The docs keep their upper-case names: `docs/USAGE.md`, `docs/BENCH.md`, `docs/SECURITY.md`, `docs/CONTRIBUTING.md`, `docs/CODE_OF_CONDUCT.md`, `docs/CHANGELOG`, `docs/ARC.toml`, `docs/ADR/` and its category folders (`docs/ADR/RETRIEVAL/`).
- Source files follow the conventions of their language (`snake_case.rs`, `snake_case.py`).
- When proposing renames or moves, list exact `mv` commands first, execute the move, fix every touched import, and run the test suite after.
- Tools for a Python move, configured in `pyproject.toml`: Rope moves a module and rewrites the imports in every source folder of the dev `sys.path` (`uv run --no-project --with rope`, no install); `pyright` (`uv tool install pyright`) checks that every import still resolves, and the errors it reports before the move name only the optional backends the dev venv does not install, plus `_urna` until the extension is copied to `python/_urna.so`; `lint-imports` (`pip install import-linter`), run from `python/` as `PYTHONPATH=. lint-imports --config ../pyproject.toml`, keeps `forge` from importing `python/tools`. None of them sees a module path written in a string, a docstring or a config file: search those with `rg`.

### Writing style

- Write in **Diataxis style**: separate tutorial, how-to, reference, and explanation. Mixing them produces noise.
- **No emoji**, anywhere. **No em-dash** (U+2014); use `,`, `;`, `.`, or a regular hyphen.
- Short paragraphs, direct voice, no marketing copy. Commits explain the **why**; the diff already shows the what. No conventional-commits prefix.
- Every governance or architecture doc starts with a YAML frontmatter block (`project`, `audience`, `status`, `last-updated`, `domain`) so LLM and vector tooling can resolve it semantically.

### Agent instruction files

`.contracts/.ai/.agents/AGENTS.md` is the shared instruction source. The repository keeps no `AGENTS.md`, `CLAUDE.md` or other per-tool file at the root: point your agent tooling at this file yourself.

Before delivery, follow the afterwork skill, `.contracts/.ai/.agents/.skills/afterwork/SKILL.md`. Its `specs.yaml` maps the files to review for each kind of change; update information that is stale and preserve files that are already correct.

### File hygiene

Organize changed code by responsibility and respect the line limit enforced by the current checks. CI checks Rust source under `crates/**/src/**`; the local gate also checks non-test Rust files under `crates/`. Documentation, generated files and data do not need splitting to meet that code limit. Keep refactoring relevant to the task and preserve behavior and test coverage.

## Code style

Rust:

- Edition 2024. `cargo fmt --all` enforced, rules pinned in `rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` is a hard gate. Suppress an individual lint with `#[allow(clippy::name)]` and a one-line justification, never globally.
- Every `unsafe` block needs a `// SAFETY:` comment naming the invariant the caller is relying on.
- Public items get a doc comment that explains the why, not the what. The name already says what.
- Keep modules focused and follow the file hygiene guidance above.

Python:

- Target `py312`, line length 100. Ruff config in `pyproject.toml`.
- Lints: `E F W I B UP SIM`. Run `sh script/ruffcheck.sh` (`URNA_PYTHON=.venv/bin/python` picks the interpreter): it checks and format-checks the one file list CI and `fullcheck.sh` share; a new Python module goes on that list.
- Private helpers in `python/tools/` use the `_` prefix (e.g. `_baseline_decoder.py`).
- Keep modules focused and follow the file hygiene guidance above.

Format and runtime invariants:

The format is frozen at v1. Any byte-level change either fits inside v1 (unused section IDs and encoding IDs are reserved and additive; the IDs already written are listed in the `contract` of `docs/ARC.toml` and named in `crates/format/src/layout/mod.rs`) or bumps `URNA_FORMAT_VERSION` and ships as v2.

## Tests

```
cargo test --release --workspace
python tests/test_pythonapi.py   # the python suites, in the order fullcheck.sh
python tests/test_ingestion.py   # runs them; that script is the list, and
python tests/test_hashguard.py   # USAGE section 10 names the three that run
python tests/test_forgespec.py   # by hand
./script/fullcheck.sh
```

The Python tests are plain scripts (`pytest tests/` does not run them). Suites that exercise the extension need a rebuilt `_urna.so`; release tooling tests have their own prerequisites. Read `fullcheck.sh` and the CI workflows for their coverage. Report skipped tests separately from passed tests and identify the tested commit.

Two lints are denied workspace-wide and will fail the build: `clippy::unwrap_used` (tests are exempt; parse paths read fields through `urna_format::bytes`) and `clippy::undocumented_unsafe_blocks` (every `unsafe` block states its invariant in a `// SAFETY:` comment). A change to any section decoder or search path should also run the mutation harness, and a new codec gets an arm in `fuzz/fuzz_targets/section_decoders.rs`:

```
cargo test -p urna-format --test mutation_fuzz -p urna-engine --test mutation_fuzz
URNA_MUTATION_ITERS=25000 cargo test --release -p urna-format --test mutation_fuzz
cargo +nightly fuzz run urna-view -- -max_total_time=600      # needs cargo-fuzz, see fuzz/README.md
```

## Reporting issues

- Bugs and feature requests: [GitHub issues](https://github.com/hoffresearch/urna/issues).
- Security vulns: do not open a public issue. Use the private advisory form (<https://github.com/hoffresearch/urna/security/advisories/new>) or email [brenner@hoffresearch.com](mailto:brenner@hoffresearch.com). Target ack within 72 hours.
- Questions about the format: open a discussion, or read `docs/ARC.toml`.

Bug reports should include the `.urna` `file_hash` and `content_hash` (from `urna stats <file>`), the runtime `simd_backend` (also in `urna stats`), the exact CLI or Python invocation, and the error output.

## Code of conduct

This project follows [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). By participating you agree to it.

## License

Contributions are licensed under the [MIT license](../LICENSE). Copyright vests in Hoff Research as the maintainer. MIT keeps your right to use, copy, modify, distribute, or sublicense your own copies of the resulting software intact.
