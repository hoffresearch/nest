---
id: ADR-0001
title: Short names of equal length for crates, folders, scripts, workflows and tests
kind: decision
status: superseded
category: STRUCTURE
date: 2026-10-04
last-updated: 2026-10-04
tags: [PACKAGING, WORKFLOWS, naming, crates]
supersedes: []
superseded-by: ADR-0002
related: ["#296", "#310", "#314", "#315", "#316", "#333", "#361", "#387", "crates/", "script/", ".github/workflows/", "tests/"]
---

# ADR-0001: Short names of equal length for crates, folders, scripts, workflows and tests

## Context

The repository had grown names of every length and style: `forge-core` at the root next to `crates/`, a CLI crate published as `urna` in `crates/urna-cli`, workflows from `ci.yml` to `release-rehearsal.yml`, scripts from `install.sh` to `stage_embedder_payload.py`, tests named after the function they once tested (`test_e2e.py`, `test_builder.py`). The team reads the tree by shape: a list where names line up is faster to scan and remember than a list of mixed lengths, and a name that says what the file covers saves opening it.

crates.io has no namespaces (RFC 3243 is accepted but cannot be used to publish), so a crate named `engine` or `bridge` is not available to us, and `crates.io/crates/urna/bridge` does not exist.

## Decision drivers

- Names of the same kind have the same length, so a listing reads as columns.
- A name says what the file covers, not how it was first written.
- Every crate directory has its package's name.
- Published names (the binary `urna`, `brew install urna`, `npm i -g @urna/cli`, the `urna` wheel, `import urna`) do not change.
- Names a tool fixes stay as the tool expects.

## Considered options

### Keep the names and document them

No churn, no broken links on published pages. The mixed lengths and the names that no longer describe their files stay.

### Bare six-letter crate names (`engine`, `bridge`)

The shortest form, matching the ADR category folders. Not publishable: crates.io has no namespaces and the bare names are taken or too generic.

### `urna-` plus six letters, nine letters for scripts, workflows and tests

Every crate is `urna-` and a six-letter word; every file in `script/`, every workflow and every test after `test_` is nine letters; top-level folders are one short word. Costs one release in which crates.io gets two new crate names and the old one-liner URL stops answering.

## Decision

The third option. Crates: `urna-format`, `urna-engine` (was `urna-runtime`), `urna-clitui` (was `urna`, in `crates/urna-cli`), `urna-bridge` (was `urna-python`), `urna-ingest` (was `forge-core` at the root; under `crates/` and kept out of the root workspace with `exclude`, since it is its own cargo workspace). Folders: `script/`, `demos/`, `packs/`, `assets/image/`; `assets/` kept its name. Workflows: `gatecheck`, `tagverify`, `wheelmake`, `pypiindex`, `pypiready`, `rustready`, `setuptest`, `rehearsal`, `runreport`, with `.github/buildprep.yml` and `.github/trustkeys`. Scripts and tests take nine-letter names (`preflight.py`, `installer.sh`, `test_hashguard.py`). `release.yml` keeps its name because cargo-dist writes it under that name; `Cargo.toml`, `README.md`, `LICENSE` and the `test_` prefix stay as their tools expect.

## Consequences

### Positive

- `crates/`, `script/`, `.github/workflows/` and `tests/` list as columns, and each name says what it covers.
- Every crate directory matches its package, so a path and a `cargo -p` name are the same word.

### Negative or accepted trade-offs

- crates.io: `urna-engine` and `urna-clitui` publish as new crates at the next release; `urna` and `urna-runtime` stay at 0.5.3, so `cargo install urna` keeps installing 0.5.3 until they are deprecated or point elsewhere.
- cargo-dist names archives after the package: `urna-clitui-<target>`. The installers try that name and fall back to `urna-<target>`.
- The one-liner moved to `script/installer.sh`. The 0.5.3 pages on crates.io, PyPI and npm print the old `scripts/install.sh` URL and image paths, which answer 404 until the next release replaces those pages.
- PyPI trusted publishing is bound to the workflow file name; the publisher was registered again for `pypiindex.yml`.

### Follow-up

- Before the next tag: the `CARGO_REGISTRY_TOKEN` must be allowed to publish new crates (`publish-new`).
- Decide whether `urna` and `urna-runtime` get a last release or a crates.io note pointing at `urna-clitui` and `urna-engine`.

## References

- Issue #296 and its sub-issues; pull requests #310, #314, #315, #316, #333 and #361 to #387.
- `docs/CONTRIBUTING.md`, Naming.
