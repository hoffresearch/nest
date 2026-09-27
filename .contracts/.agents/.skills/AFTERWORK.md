---
name: afterwork
description: run at the end of every task, before the pull request. walks the files a change owns and brings each one up to date in place.
project: urna
audience: ai agents and human contributors
status: active
last-updated: 2026-09-27
domain: workflow
---

# afterwork

a change is done when the tree describes it. walk the list below against the diff, open every file a block names, and fix what the change made wrong or incomplete. skip a block the diff does not touch.

edit in place, as if the file had always said the right thing. history lives in three places only: `docs/CHANGELOG`, the commit message and the pull request body. no "renamed x to y" or "updated for the new flow" notes anywhere else, and no edits to files the change does not affect.

## every change

- `docs/CHANGELOG`: an `[Unreleased]` entry with the why and the measured numbers. this is also where a decision and its reasoning are recorded (the project's decision log). test counts in it match what runs.
- `docs/arc/ARC.toml`: the one architecture reference. update it when a module, boundary, flow, public contract, storage or runtime behavior changes; a new tracked file gets an `inventory` entry. bump `last-updated`, append a dated note to `summary`.
- `.contracts/.agents/AGENTS.md`: the one instruction source (the root `CLAUDE.md` is a symlink to it; never edit the link or add a parallel file). update it when a command, gotcha, known gap, test count or layout changes.
- tests next to the code (`crates/*/tests/`, `tests/*.py`, `python/forge/test_*.py`): happy path, error path, one edge case, against real artifacts, no mocks.
- `scripts/release_check.sh`: run it, do not edit it to pass. it rebuilds `python/_urna.so` after rust changes.

## user-visible behavior

- `README.md`: cli surface, python api, presets, benchmarks, install.
- `docs/USAGE.md`: the how-to per verb, preset, declarative build, model registry, setup/tui and install channel. a new feature usually gets a section or changes one.
- `examples/`: the quickstart, fastapi, flask and jupyter examples still run.
- `assets/images/`: the `urna setup` / `urna tui` screenshots after a ui change (render recipe in the AGENTS.md gotchas).

## binary format or decoders

- `crates/urna-format/tests/`: a new section or codec gets a roundtrip and a `negative_*.rs`.
- `crates/urna-format/tests/fixtures/`: the golden is regenerated with `regen_golden` only when the format really changed, which in v1 is almost never.
- `fuzz/fuzz_targets/section_decoders.rs`: a new codec gets an arm; a fuzz finding becomes `fuzz/seeds/regress-*.bin` and a `negative_*.rs`.
- the section-id map in ARC.toml's `contract` array: new sections are additive, no `URNA_FORMAT_VERSION` bump.
- `data/measure/baseline.json` and its neighbors: regression baselines. update only for an intended, measured change.
- `docs/BENCH.md`: regenerate with `python/tools/bench_competitors.py`, never by hand.

## python, packaging, release

- `scripts/ruff_check.sh`: the one python file list shared by ci and release_check. a new module goes here.
- `packaging/pyproject.toml`: the one source of the wheel. `packaging/staging/` is generated.
- `pyproject.toml` (root): dependency groups and the ruff config.
- `Cargo.toml` (workspace): the version tracks the latest tag; msrv 1.85, the cli crate 1.88. after editing the dist config run `dist generate`.
- `.github/workflows/*.yml`: a new install channel or release artifact gets a job in `install-test.yml`. read item 11 of the maintainer checklist in `docs/USAGE.md` before touching `release.yml` or `pypi.yml`.

## security and data

- `docs/SECURITY.md`: hardening, network surface (the binary links no network stack; keep it that way) and data posture.
- `scripts/pre-commit`: a new kind of data artifact that may be committed goes on the allow-list.
- `.gitattributes`: a new large binary (`*.urna`, safetensors, datasets) goes to lfs.

## rarely

- `docs/CONTRIBUTING.md`: only when the contribution flow (branch, pr, gate) changes.
- `data/demo/Instructions.md`: only when an upstream dataset or the corpus rebuild changes.
- `.editorconfig`: only for a new language or file type.

## file hygiene

every file the change created or grew:

- over 639 lines: read what it does, what it depends on and who imports it, then split it by responsibility into modules that each do one thing. update every import and caller, and keep the public surface where it was. the tests pass before and after, with the same count. exempt: tests, data and generated files, lockfiles, json, yaml, toml, ron, jsonl, csv, datasets, vendored files.
- dead weight: temporary scripts, logs, backups, stray files, code nothing calls. check that nothing imports it, delete it, run the tests.
- misplaced: a file outside the folder its role belongs to per ARC.toml, or named against the repo's style (kebab-case dirs and docs, language-idiomatic sources). move or rename it (`git mv`, `git mv -f` for a case-only rename), fix every reference, list it in ARC.toml's inventory.
- in the code: machine-specific paths and hardcoded values that should come from config or env, and comments that are verbose, stale or narrate history. fix or trim them.

## lessons

something that failed first and then found its fix is worth a line for the next person:

- a dev or agent trap goes to the AGENTS.md `gotchas`;
- a release or ops lesson goes to the maintainer checklist in `docs/USAGE.md` (item 11);
- a decision goes to its `docs/CHANGELOG` entry, with the why.

no per-session notes files; the lesson lives where the next person will look.

## before the pull request

- the file hygiene block above is done: no file over its limit, nothing dead or misplaced left.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, ruff: clean.
- `forge-core` tested on its own manifest (`--workspace` does not reach it).
- no temporary script, stray file (a package manager run from the repo root leaves a `package.json`), dead code or orphan import left; `git status` is clean apart from the change.
- no hardcoded machine paths; comments short and current.
- docs lowercase, no emoji, no em dash.
- local and remote in sync: merged branches deleted, nothing unpushed.
- one pull request per topic, in english: the title says what changes, the body says why and how it was tested.
