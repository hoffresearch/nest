# contributing

`nest` is maintained by [hoff research](https://hoffresearch.com). author: brenner cruvinel ([brenner@hoffresearch.com](mailto:brenner@hoffresearch.com)). all contributions are welcome.

## how to contribute

1. fork the repo at https://github.com/hoffresearch/nest.
2. branch from `dev`: `git checkout -b feature/short-description`.
3. keep each pr focused on one concern. small is better.
4. add or update tests for the change. new behavior needs a new test.
5. run `./scripts/release_check.sh` locally before pushing. that script is the same gate ci runs on the pr.
6. commit with a clear message in plain english. no conventional commits prefix.
7. open a pr against `dev`. the maintainer squashes or rebases into `main` at release time.

## setup

requires rust edition 2024 (`rustc >= 1.85`) and python 3.12+.

```
git clone https://github.com/hoffresearch/nest.git
cd nest

cargo build --release --workspace
cp target/release/lib_nest.dylib python/_nest.so   # macOS
cp target/release/lib_nest.so   python/_nest.so    # linux

python3 -m venv .venv && source .venv/bin/activate
pip install ruff sentence-transformers pandas zstandard pyarrow
```

`database/` and `data/corpus_next.v1.nest` are tracked via git lfs. first checkout pulls ~600 mb. without lfs the public datasets are skipped, the runtime tests still pass.

## code style

rust:

- edition 2024. `cargo fmt --all` enforced, rules pinned in `rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` is a hard gate. suppress an individual lint with `#[allow(clippy::name)]` and a one-line justification, never globally.
- every `unsafe` block needs a `// SAFETY:` comment naming the invariant the caller is relying on.
- public items get a doc comment that explains the why, not the what. the name already says what.
- files in `crates/**/src/**` cap at 300 lines, see `kdb/adr/0011`. test files are exempt.

python:

- target `py312`, line length 100. ruff config in `pyproject.toml`.
- lints: `E F W I B UP SIM`. run `ruff check .` and `ruff format --check .`.
- private helpers in `python/tools/` use the `_` prefix, e.g. `_baseline_decoder.py`.
- same 300-line cap as rust.

format and runtime invariants:

if a change touches the binary container layout, the search contract, hash semantics, or the model fingerprint, write or update an adr under `kdb/adr/`. the format is frozen at v1. any byte-level change either fits inside v1 (new section ids and encodings 4-255 are reserved) or bumps `NEST_FORMAT_VERSION` and ships as v2.

## tests

```
cargo test --release --workspace
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
./scripts/release_check.sh
```

`release_check.sh` is the source of truth. if it passes locally, ci passes.

## reporting issues

- bugs and feature requests: [github issues](https://github.com/hoffresearch/nest/issues).
- security vulns: do not open a public issue. email [brenner@hoffresearch.com](mailto:brenner@hoffresearch.com). target ack within 72 hours.
- questions about the format: open a discussion, or read `doc/spec.md` and `kdb/adr/`.

bug reports should include the `.nest` `file_hash` and `content_hash` (from `nest stats <file>`), the runtime `simd_backend` (also in `nest stats`), the exact cli or python invocation, and the error output.

## code of conduct

this project follows [code_of_conduct.md](CODE_OF_CONDUCT.md). by participating you agree to it.

## license

contributions are licensed under the [mit license](LICENSE). copyright vests in hoff research as the maintainer. mit keeps your right to use, copy, modify, distribute, or sublicense your own copies of the resulting software intact.
