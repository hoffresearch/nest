---
name: afterwork
description: run at the end of every task, before the pull request. walks the files a change owns and brings each one up to date in place.
project: urna
audience: ai agents and human contributors
status: active
last-updated: 2026-10-02
domain: workflow
---

# Afterwork

A change is done when the tree describes it. Walk the list below against the diff, open every file a block names, and fix what the change made wrong or incomplete. Every item applies only when the diff touches what it names: a docs-only change skips the builds and the gate, a change outside the format skips the format block.

Edit in place, as if the file had always said the right thing. History lives in three places only: `docs/CHANGELOG`, the commit message and the pull request body. No "renamed x to y" or "updated for the new flow" notes anywhere else, and no edits to files the change does not affect.

## Code and behavior

- `docs/CHANGELOG`: an `[Unreleased]` entry for anything a user, an operator or a contributor would notice, with the why and the measured numbers. This is also where a decision and its reasoning are recorded (the project's decision log). Test counts in it match what runs.
- `docs/arc/ARC.toml`: the one architecture reference. Update it when a module, boundary, flow, public contract, storage or runtime behavior changes; a new tracked file gets an `inventory` entry. Bump `last-updated`, append a dated note to `summary`.
- `.contracts/.agents/AGENTS.md`: the one instruction source (the root `CLAUDE.md` is a symlink to it; never edit the link or add a parallel file). Update it when a command, gotcha, known gap, test count or layout changes.
- Tests next to the code it changes (`crates/*/tests/`, `tests/*.py`, `python/forge/test_*.py`): happy path, error path, one edge case, against real artifacts, no mocks.
- `scripts/release_check.sh`: run it for any code change, do not edit it to pass; it rebuilds `python/_urna.so` after Rust changes. A docs-only change does not need it, and the pull request says it was not run.

## User-visible behavior

- `README.md`: CLI surface, Python API, presets, benchmarks, install. `llms.txt`: its links and one-line summaries.
- `docs/USAGE.md`: the how-to per verb, preset, declarative build, model registry, setup/TUI and install channel. A new feature usually gets a section or changes one.
- `examples/`: the quickstart, fastapi, flask and jupyter examples still run.
- `assets/images/`: the `urna setup` / `urna tui` screenshots after a UI change (render recipe in the AGENTS.md gotchas).

## Binary format or decoders

- `crates/urna-format/tests/`: a new section or codec gets a roundtrip and a `negative_*.rs`.
- `crates/urna-format/tests/fixtures/`: the golden is regenerated with `regen_golden` only when the format really changed, which in v1 is almost never.
- `fuzz/fuzz_targets/section_decoders.rs`: a new codec gets an arm; a fuzz finding becomes `fuzz/seeds/regress-*.bin` and a `negative_*.rs`.
- The section-id map in ARC.toml's `contract` array: new sections are additive, no `URNA_FORMAT_VERSION` bump.
- `data/measure/baseline.json` and its neighbors: regression baselines. Update only for an intended, measured change.
- `docs/BENCH.md`: regenerate with `python/tools/bench_competitors.py`, never by hand.

## Python, packaging, release

- `scripts/ruff_check.sh`: the one Python file list shared by CI and release_check. A new module goes here.
- `packaging/pyproject.toml`: the one source of the wheel. `packaging/staging/` is generated.
- `pyproject.toml` (root): dependency groups and the ruff config.
- `Cargo.toml` (workspace): the version tracks the latest tag; MSRV 1.85, the CLI crate 1.88. After editing the dist config run `dist generate`.
- `.github/workflows/*.yml`: a new install channel or release artifact gets a job in `install-test.yml`. Read item 11 of the maintainer checklist in `docs/USAGE.md` before touching `release.yml` or `pypi.yml`.

## Security and data

- `docs/SECURITY.md`: hardening, network surface (the binary links no network stack; keep it that way) and data posture.
- `scripts/pre-commit`: a new kind of data artifact that may be committed goes on the allow-list.
- `.gitattributes`: a new large binary (`*.urna`, safetensors, datasets) goes to LFS.

## Rarely

- `docs/CONTRIBUTING.md`: only when the contribution flow (branch, PR, gate) changes.
- `data/demo/Instructions.md`: only when the gate corpus or an image source changes.
- `.editorconfig`: only for a new language or file type.

## File hygiene

Every file the change created or grew:

- Over 639 lines: read what it does, what it depends on and who imports it, then split it by responsibility into modules that each do one thing. Update every import and caller, and keep the public surface where it was. The tests pass before and after, with the same count. Exempt: tests, data and generated files, lockfiles, JSON, YAML, TOML, RON, JSONL, CSV, datasets, vendored files.
- Dead weight: temporary scripts, logs, backups, stray files, code nothing calls. Check that nothing imports it, delete it, run the tests.
- Misplaced: a file outside the folder its role belongs to per ARC.toml, or named against the repo's style (kebab-case dirs and docs, language-idiomatic sources). Move or rename it (`git mv`, `git mv -f` for a case-only rename), fix every reference, list it in ARC.toml's inventory.
- In the code: machine-specific paths and hardcoded values that should come from config or env, and comments that are verbose, stale or narrate history. Fix or trim them.

## Lessons

Something that failed first and then found its fix is worth a line for the next person:

- A dev or agent trap goes to the AGENTS.md `gotchas`;
- A release or ops lesson goes to the maintainer checklist in `docs/USAGE.md` (item 11);
- A decision goes to its `docs/CHANGELOG` entry, with the why.

No per-session notes files; the lesson lives where the next person will look.

## Before the pull request

- The file hygiene block above is done: no file over its limit, nothing dead or misplaced left.
- For a code change: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, ruff: clean.
- `forge-core`, when the change touches it: tested on its own manifest (`--workspace` does not reach it).
- No temporary script, stray file (a package manager run from the repo root leaves a `package.json`), dead code or orphan import left; `git status` is clean apart from the change.
- No hardcoded machine paths; comments short and current.
- Docs: no emoji, no em dash.
- Local and remote in sync: merged branches deleted, nothing unpushed.
- One pull request per topic, in English: the title says what changes, the body says why and how it was tested.
