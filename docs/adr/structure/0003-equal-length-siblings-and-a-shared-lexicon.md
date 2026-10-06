---
id: ADR-0003
title: Equal-length siblings at every level, with a shared abbreviation lexicon
kind: decision
status: accepted
category: STRUCTURE
date: 2026-10-06
last-updated: 2026-10-06
tags: [WORKFLOWS, PACKAGING, naming, python]
supersedes: [ADR-0002]
superseded-by: null
related: ["#296", "#479", "tool/tasks/namecheck.py", "docs/TERMS.md", "rust/bridge/python/urna/"]
---

# ADR-0003: Equal-length siblings at every level, with a shared abbreviation lexicon

## Context

ADR-0001 introduced the equal-length rule and ADR-0002 carried it forward ("the rest of ADR-0001 stands"): crate folders are six letters, scripts, workflows and tests nine. The rule covered those levels only. The root still mixed lengths (`crates`, `script`, `tests`, `python`, `packs`, `demos`, `assets`), `python/forge/` grouped its modules by prefix (`forge_`, `image_`, `model_`) instead of by package, and nothing checked the rule, so it held only as long as every reviewer remembered it.

A tree that reads by shape helps everyone who scans a listing, and autistic and ADHD developers most: a column of equal names is parsed once, a ragged one on every visit. The rule has to reach every level without forcing a name a tool or a distribution channel fixes.

## Decision drivers

- Siblings of the same kind read as one column.
- Every level says what length its children take, so a checker neither guesses nor skips one.
- A name a tool, a channel or a project convention fixes stays as it is.
- An abbreviation means one thing everywhere it appears.
- Published names stay: the binary `urna`, the crates.io packages, the `urna` wheel, `import urna`.

## Considered options

### Keep ADR-0002 and fix names as they are touched

No move now. The root stays ragged, `python/forge/` keeps its prefixes and the rule stays unchecked.

### One rule for every level, a length table, a lexicon and a checker

Move the tree once, write the length of each level down, list the abbreviations, and check both in CI.

### Free names with a style guide

Readable names, no length rule. Simple to follow, but it gives up the column reading the rule exists for.

## Decision

One rule: **siblings of the same kind have the same length.** A length counts letters and digits only: no hyphen, no underscore, no extension, no leading dot of a hidden folder and no `test_` prefix (`av1stream.py` and `potionb8m/` are 9). Folders, modules, scripts and helper files are lowercase; the project docs in `docs/` are uppercase, like `README.md` and `LICENSE`.

| Where | Children | Length |
| --- | --- | --- |
| root | folders | 4 (`data`, `demo`, `docs`, `fuzz`, `pkgs`, `rust`, `tool`) |
| root | hidden folders | 6 (`.devops`, `.github`, `.pdteam`, `.secops`) |
| `rust/` | crates | 6 (`bridge`, `clitui`, `engine`, `format`, `ingest`) |
| `rust/bridge/python/urna/` | subpackages | 5 (`embed`, `entry`, `gates`, `image`, `model`, `pipes`, `reads`, `specs`) |
| `urna/<subpackage>/` | modules | 9 (`potiontab.py`, `neighbors.py`...) |
| `urna/model/` | data folders | 9 (`potionb8m`) |
| `tool/` | folders | 5 (`bench`, `tasks`, `tests`) |
| `tool/bench/`, `tool/tasks/` | scripts | 9 (`benchgate.py`, `namecheck.py`...) |
| `tool/tests/` | tests | `test_` + 9 |
| `demo/` | folders | 7 (`corpora`, `fastapi`, `flaskpy`, `jupyter`, `starter`) |
| `demo/corpora/` | folders | 5 (`intro`, `legal`, `photo`...) |
| `docs/` | folders | 3 (`adr`, `img`) |
| `docs/` | project docs | 5 (`ARCHS.toml`, `BENCH.md`, `TERMS.md`, `USAGE.md`) |
| `docs/adr/` | categories | 9 (`container`, `structure`...) |
| `pkgs/` | channels | 5 (`choco`, `conda`, `linux`, `nixos`, `scoop`, `wheel`, `wingt`) |
| `pkgs/linux/` | families | 3 (`apt`, `aur`, `dnf`) |
| `.devops/` | folders | 5 (`agent`, `rules`) |
| `.devops/agent/` | folders | 5 (`skill`) |
| `.devops/agent/skill/` | skills | 9 (`afterwork`, `benchsync`, `docscheck`, `factcheck`, `releasing`) |
| `.devops/agent/skill/<skill>/` | helper files | 5 (`specs.yaml`, `bench.yaml`) |
| `.devops/rules/` | folders | 6 (`mantra`, `taboos`) |
| `.github/` | files | 9 (`buildprep.yml`, `trustkeys`) |
| `.github/workflows/` | workflows | 9 (`gatecheck.yml`...) |

Names outside the rule:

| Kind | Names |
| --- | --- |
| Fixed by a tool | `.zed/` (the Zed editor's project settings), `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/`, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `release.toml`, `pyproject.toml`, `Dockerfile`, `README.md`, `LICENSE`, `CHANGELOG`, `CITATION.cff`, `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `SECURITY.md`, `release.yml` (cargo-dist), `pull_request_template.md`, `AGENTS.md`, `SKILL.md`, `__init__.py`, `_urna.so` (maturin `module-name`), `python/` (maturin `python-source`), `urna/` (the published `import urna`), `fuzz_targets/` (cargo-fuzz), `.github/workflows/` (GitHub Actions), `.gitkeep`, the files inside `potionb8m/` (the upstream model's names) |
| Fixed by a distribution channel | `urna.nuspec`, `tools/chocolateyInstall.ps1`, `recipe.yaml`, `debian/` and its `control`, `rules`, `changelog`, `copyright`, `source/format`, `PKGBUILD`, `urna.spec`, `nfpm.yaml`, `package.nix`, `urna.json`, the three `HoffResearch.Urna.*.yaml` winget manifests |
| Project convention | `docs/adr/TEMPLATE.md`, `docs/adr/README.md`, ADR records (`NNNN-slug.md`), `fuzz/seeds/` |

An exception is a place, not a name: `namecheck.py` exempts a name only at the path its tool, channel or convention puts it (`SKILL.md` inside a skill, `release.yml` in `.github/workflows/`), and the same name anywhere else is held to the rule.

`docs/TERMS.md` holds the abbreviation lexicon, with three rules: one abbreviation, one meaning; no synonym invented to fill characters, the project's vocabulary first; a new abbreviation enters `TERMS.md` in the pull request that creates the name.

`tool/tasks/namecheck.py` reads the tracked files (`git ls-files`) and checks the table, the exceptions and the lexicon; `fullcheck.sh` and `gatecheck.yml` run it, so the convention is enforced, not only written down. Generated and ignored folders (`target/`, `.venv/`, `pkgs/stage/`) are out of its scope.

`python/forge/` dissolves into the `urna` package at `rust/bridge/python/urna/`: its prefixes become subpackages and leave the module names. Two consequences follow from one package:

- `urna/__init__.py` loads the `_urna` extension on first use, not on import, because the query embedders of `urna/embed/` and `urna/model/` run in the release payload, which carries no extension, and importing them imports the package first. `embed/__init__.py` re-exports its embedders the same way, so a query script does not inherit the offline mode `potiontab` forces when it is imported.
- The payload installs the package as it is in the repository, under `<root>/urna/python/urna/`, beside `VERSION`.

## Consequences

### Positive

- The root lists seven four-letter folders; every level of the table reads as a column.
- The convention is checked in CI on every pull request, together with the lexicon.
- One Python package: imports say where a module lives (`urna.model.presetmap`), and the payload, the wheel and the checkout share one layout.

### Negative or accepted trade-offs

- An existing install needs `urna setup` again: the payload moves from `<root>/urna/forge/` and the top-level modules to `<root>/urna/python/urna/`; setup and the one-liner installers remove the old files.
- The wheel's module paths follow the package at the next release: `urna.embed_potion` is `urna.embed.potiontab`, and the console script runs `urna.entry.clidriver`.
- `import urna` succeeds without the extension; a missing `_urna` surfaces at the first `urna.open` or `urna.UrnaFile`, with the same message as before.
- Abbreviations (`qry`, `emb`, `cmp`) have to be learned once; the lexicon is where they are.
- The README, crates.io, PyPI and npm pages of 0.5.4 print `script/installer.sh`, which answers 404 until the next release replaces those pages; `script/` left with this move, without forwarding stubs.

### Follow-up

- Decide the name of `docscheck` (check if it only verifies, sync if it writes, as `benchsync`) when its `SKILL.md` is written, inside the nine-character rule.
- Fill the manifests of `pkgs/` (choco, conda, linux, nixos, scoop, wingt), in their own issue.

## References

- ADR-0002, superseded by this record; ADR-0001, which it superseded.
- Issue #479, part of #296.
- `docs/TERMS.md`, `tool/tasks/namecheck.py`, `docs/CONTRIBUTING.md` (Naming).
