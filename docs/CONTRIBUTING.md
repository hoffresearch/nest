---
project: urna
audience: contributors
status: active
last-updated: 2026-10-04
domain: contributing
---

# Contributing

`urna` is maintained by [Hoff Research](https://hoffresearch.com). Author: Brenner Cruvinel ([brenner@hoffresearch.com](mailto:brenner@hoffresearch.com)). All contributions are welcome.

## How to contribute

1. Fork the repo at https://github.com/hoffresearch/urna.
2. Branch from `main`: `git checkout -b feature/short-description origin/main`.
3. Keep each PR focused on one concern. Small is better.
4. Add or update tests for the change. New behavior needs a new test. Write real tests against real artifacts (built .urna files, golden fixtures, real corpora), no mocks; cover the happy path, the error path, and one edge case.
5. If the change alters architecture, module boundaries, data flow, or doc locations, update `docs/arc/ARC.toml` in the same PR. Keep it concise and pragmatic. Do not add a separate human architecture doc; `ARC.toml` is the machine map, the human reference, and the mermaid diagram all in one file.
6. For a code change, run `./scripts/release_check.sh` locally before pushing; a docs-only change skips it and says so in the PR. It is the gate (the Rust suite in release, the extension rebuilt, the Python suites, ruff, the regression gates against `data/measure/baseline.json`). `.github/workflows/ci.yml` covers the Rust side on Linux, macOS and Windows plus checks the local gate does not run (cargo-deny, cargo-semver-checks, the engine-only Clippy, the benches compiled, a cargo-fuzz smoke, the embedder payload staged under Python 3.10). Its Python job runs ruff, the model catalog check and the release suites that need no built extension; the suites that load `_urna.so` run only in the local gate, so run them locally. A pull request that touches a release input also runs the release rehearsal (`release-rehearsal.yml`), a required check.
7. Commit with a clear message in plain English. No conventional commits prefix.
8. Open a PR against `main`. The maintainer squash merges it; `main` requires verified (SSH-signed) commits, linear history and a passing `rehearsal` check (the release rehearsal, dispensed when the change touches no release input), so sign your commits (`git config commit.gpgsign true` with an SSH or GPG key registered on GitHub).

## Setup

Requires Rust edition 2024 (`rustc >= 1.88`: the CLI crate's floor, set by Ratatui; the format, runtime and Python crates alone build on 1.85) and Python 3.12+.

```
git clone https://github.com/hoffresearch/urna.git
cd urna

cargo build --release --workspace
# the python extension is a separate build: `pyo3/extension-module` keeps
# libpython out of the cdylib (without it the .so segfaults under uv's
# standalone interpreters)
cargo build --release -p urna-python --features pyo3/extension-module
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

- Directories and assets are **kebab-case English** (`data/`, `docs/`, `examples/`, `assets/images/`, dataset folders); Rust workspace conventions (`crates/`, `target/`) and language defaults (`python/`, `scripts/`, `tests/`) stay as their stacks expect.
- The docs keep their upper-case names: `docs/USAGE.md`, `docs/BENCH.md`, `docs/SECURITY.md`, `docs/CONTRIBUTING.md`, `docs/CODE_OF_CONDUCT.md`, `docs/CHANGELOG`, `docs/LICENSE`, `docs/arc/ARC.toml`.
- Source files follow the conventions of their language (`snake_case.rs`, `snake_case.py`).
- When proposing renames or moves, list exact `mv` commands first, execute the move, fix every touched import, and run the test suite after.

### Writing style

- Write in **Diataxis style**: separate tutorial, how-to, reference, and explanation. Mixing them produces noise.
- **No emoji**, anywhere. **No em-dash** (`-`); use `,`, `;`, `.`, or a regular hyphen.
- Short paragraphs, direct voice, no marketing copy. Commits explain the **why**; the diff already shows the what. No conventional-commits prefix.
- Every governance or architecture doc starts with a YAML frontmatter block (`project`, `audience`, `status`, `last-updated`, `domain`) so LLM and vector tooling can resolve it semantically.

### Agent instruction files

- `.contracts/.agents/AGENTS.md` is the single instruction source for AI coding agents working in this repo: use/update/init only `.contracts/.agents/AGENTS.md` (the core global agent file).
- Do not create per-tool instruction files (GEMINI.md, CODEX.md, cursor rules). The root `CLAUDE.md` is a symlink to that file, not a second source; most agentic tooling already reads `.contracts/.agents/AGENTS.md` by default, point the rest at it on init.

### File hygiene

Human working memory holds four plus or minus one chunks at once (Cowan, 2001). Neural networks behave better the same way. A file that does not fit the mental window forces internal context switching and raises bug rates. This is the same principle UI designers apply to information density.

- **Hard limit: 639 lines per code file.** Above it, split along single-responsibility lines in the same PR.
- Exempt: tests, data and generated files, lockfiles, JSON, YAML, TOML and vendored files.

## Code style

Rust:

- Edition 2024. `cargo fmt --all` enforced, rules pinned in `rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` is a hard gate. Suppress an individual lint with `#[allow(clippy::name)]` and a one-line justification, never globally.
- Every `unsafe` block needs a `// SAFETY:` comment naming the invariant the caller is relying on.
- Public items get a doc comment that explains the why, not the what. The name already says what.
- File hygiene as above: 639 lines.

Python:

- Target `py312`, line length 100. Ruff config in `pyproject.toml`.
- Lints: `E F W I B UP SIM`. Run `sh scripts/ruff_check.sh` (`URNA_PYTHON=.venv/bin/python` picks the interpreter): it checks and format-checks the one file list CI and `release_check.sh` share; a new Python module goes on that list.
- Private helpers in `python/tools/` use the `_` prefix (e.g. `_baseline_decoder.py`).
- File hygiene as above: 639 lines.

Format and runtime invariants:

The format is frozen at v1. Any byte-level change either fits inside v1 (unused section IDs and encoding IDs are reserved and additive; the IDs already written are listed in the agents contract and named in `crates/urna-format/src/layout/mod.rs`) or bumps `URNA_FORMAT_VERSION` and ships as v2.

## Tests

```
cargo test --release --workspace
python tests/test_e2e.py                      # the python suites, in the order
python tests/test_builder.py                  # release_check.sh runs them; that
python tests/test_search_text_model_hash.py   # script is the list, AGENTS.md names
python tests/test_forge_spec.py               # the three that run by hand
./scripts/release_check.sh
```

The Python tests are plain scripts (`pytest tests/` does not work) and need the built `_urna.so`. `release_check.sh` is the source of truth for the Python side; CI runs the Rust gates plus deny, semver and the Windows job on top of it, so a green local gate is necessary, not sufficient.

Two lints are denied workspace-wide and will fail the build: `clippy::unwrap_used` (tests are exempt; parse paths read fields through `urna_format::bytes`) and `clippy::undocumented_unsafe_blocks` (every `unsafe` block states its invariant in a `// SAFETY:` comment). A change to any section decoder or search path should also run the mutation harness, and a new codec gets an arm in `fuzz/fuzz_targets/section_decoders.rs`:

```
cargo test -p urna-format --test mutation_fuzz -p urna-runtime --test mutation_fuzz
URNA_MUTATION_ITERS=25000 cargo test --release -p urna-format --test mutation_fuzz
cargo +nightly fuzz run urna-view -- -max_total_time=600      # needs cargo-fuzz, see fuzz/README.md
```

## Reporting issues

- Bugs and feature requests: [GitHub issues](https://github.com/hoffresearch/urna/issues).
- Security vulns: do not open a public issue. Use the private advisory form (<https://github.com/hoffresearch/urna/security/advisories/new>) or email [brenner@hoffresearch.com](mailto:brenner@hoffresearch.com). Target ack within 72 hours.
- Questions about the format: open a discussion, or read `docs/arc/ARC.toml`.

Bug reports should include the `.urna` `file_hash` and `content_hash` (from `urna stats <file>`), the runtime `simd_backend` (also in `urna stats`), the exact CLI or Python invocation, and the error output.

## Code of conduct

This project follows [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). By participating you agree to it.

## License

Contributions are licensed under the [MIT license](LICENSE). Copyright vests in Hoff Research as the maintainer. MIT keeps your right to use, copy, modify, distribute, or sublicense your own copies of the resulting software intact.
