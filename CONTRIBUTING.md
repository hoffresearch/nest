# Contributing to nest

Thanks for your interest in improving `nest`. All contributions are valued.

`nest` is maintained by [Hoff Research](https://hoffresearch.com). Author and current maintainer: Brenner Cruvinel ([brenner@hoffresearch.com](mailto:brenner@hoffresearch.com)).

## How to contribute

1. Fork the repository at https://github.com/hoffresearch/nest.
2. Create a feature branch from `dev`:
   `git checkout -b feature/short-description`.
3. Make your changes, keeping each PR focused on one concern.
4. Add or update tests for the change. The bar is `cargo test --workspace` and `python tests/test_e2e.py` both green; new behavior needs a new test.
5. Run `./scripts/release_check.sh` locally before pushing. It runs the full pipeline (build, test, clippy, fmt, ruff, line-count guard, regression gates) and is what CI runs on the PR.
6. Commit with a descriptive message. We use plain English, no Conventional Commits prefix.
7. Open a Pull Request against `dev`. The maintainer rebases or squashes into `main` at release time.

## Development setup

Requires Rust edition 2024 (`rustc >= 1.85`) and Python 3.12+.

```bash
git clone https://github.com/hoffresearch/nest.git
cd nest

# build the rust workspace + PyO3 extension
cargo build --release --workspace
cp target/release/lib_nest.dylib python/_nest.so   # macOS
cp target/release/lib_nest.so   python/_nest.so    # linux

# python deps for the build/test side
python3 -m venv .venv && source .venv/bin/activate
pip install ruff sentence-transformers pandas zstandard pyarrow
```

The corpus and embedding cache under `database/` are tracked via Git LFS. Expect a one-time `git lfs pull` of around 600 MB on first checkout. Without LFS the public datasets are skipped, the runtime tests still pass.

## Code style

### Rust

- Edition 2024, `cargo fmt --all` enforced (`rustfmt.toml` pins the rules).
- `cargo clippy --workspace --all-targets -- -D warnings` is a hard error gate. Suppress an individual lint with `#[allow(clippy::name_of_lint)]` and a one-line justification, never with a global allow.
- Every `unsafe` block needs a `// SAFETY:` comment that names the invariant the caller is relying on.
- Public items get a doc comment that explains the why, not the what. The function name already says what.
- Files in `crates/**/src/**` cap at 300 lines (see `kdb/adr/0011`). Test files (`crates/**/tests/*.rs`) are exempt.

### Python

- Target version `py312`, line length 100, ruff config in `pyproject.toml`.
- Lints: `E F W I B UP SIM`. Run `ruff check .` and `ruff format --check .`.
- Private helpers in `python/tools/` are prefixed with `_` (e.g. `_baseline_decoder.py`) to signal "internal to tools, do not import elsewhere".
- Same 300-line cap applies to first-party modules.

### Format / runtime invariants

If a change touches the binary container layout, the search contract, hash semantics, or the model fingerprint, write or update an ADR under `kdb/adr/`. The format is frozen at v1; any byte-level change has to either fit inside v1 (new section IDs are reserved, encodings 4-255 are reserved) or bump `NEST_FORMAT_VERSION` and ship as v2.

## Tests

```bash
cargo test --release --workspace
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
./scripts/release_check.sh
```

`release_check.sh` is the source of truth for "PR-ready". If it passes locally, CI passes.

## Reporting issues

- Bugs and feature requests: [GitHub Issues](https://github.com/hoffresearch/nest/issues).
- Security vulnerabilities: please do **not** open a public issue. Email [brenner@hoffresearch.com](mailto:brenner@hoffresearch.com) directly. We aim to acknowledge within 72 hours.
- Questions about the format itself: open a discussion or check `SPEC.md` and `kdb/adr/`.

When reporting a bug, include:
- the `.nest` `file_hash` and `content_hash` (`nest stats <file>`),
- the runtime `simd_backend` (`nest stats` prints it),
- the exact CLI or Python invocation, and the error output.

## Code of Conduct

This project adheres to a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold it.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE) and that copyright vests in Hoff Research as the project maintainer. The MIT license keeps your right to use, copy, modify, merge, publish, distribute, sublicense, or sell your own copies of the resulting Software intact.
