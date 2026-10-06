---
id: ADR-0002
title: Short folder names; published package names keep the urna prefix
kind: decision
status: accepted
category: STRUCTURE
date: 2026-10-04
last-updated: 2026-10-04
tags: [PACKAGING, WORKFLOWS, naming, crates]
supersedes: [ADR-0001]
superseded-by: null
related: ["#296", "#391", "#392", "#393", "#394", "#395", "crates/", "script/", ".github/workflows/", "tests/"]
---

# ADR-0002: Short folder names; published package names keep the urna prefix

## Context

ADR-0001 gave every crate folder the `urna-` prefix and renamed the CLI package from `urna` to `urna-clitui`. The prefix is only needed on a published package name, because crates.io has no namespaces (RFC 3243 is accepted but cannot be used to publish). A folder under `crates/` can have any name: Cargo reads the package name from `Cargo.toml`, not from the folder. Renaming the CLI package would have moved `cargo install urna` to a new crate and left the `urna` page on crates.io at 0.5.3.

## Decision drivers

- Names of the same kind have the same length, so a listing reads as columns.
- A name says what the file covers.
- Published names stay: the binary `urna`, `cargo install urna`, `brew install urna`, `npm i -g @urna/cli`, the `urna` wheel, `import urna`.
- Names a tool fixes stay as the tool expects.

## Decision

Crate folders are six letters with no prefix; packages keep the `urna-` prefix where they are published, and the CLI package is `urna`:

| Folder | Package |
| --- | --- |
| `crates/format` | `urna-format` |
| `crates/engine` | `urna-engine` |
| `crates/clitui` | `urna` (binary `urna`) |
| `crates/bridge` | `urna-bridge` (`publish = false`, ships as the `urna` wheel) |
| `crates/ingest` | `urna-ingest` (`publish = false`, its own cargo workspace, excluded at the root) |

The rest of ADR-0001 stands: files in `script/`, workflows and tests after `test_` are nine letters (`preflight.py`, `gatecheck.yml`, `test_hashguard.py`); top-level folders are one short word (`script/`, `demos/`, `packs/`, `assets/image/`); `release.yml`, `Cargo.toml`, `README.md`, `LICENSE` and the `test_` prefix keep the names their tools expect.

## Consequences

### Positive

- `crates/` lists five six-letter folders.
- `cargo install urna`, `cargo binstall urna` and the release archive names (`urna-<target>`) are unchanged from 0.5.3; the installers need no fallback.

### Negative or accepted trade-offs

- A folder and its package no longer share a name: `cargo -p urna-engine` builds `crates/engine`.
- `urna-engine` is still a rename of `urna-runtime`: the next release publishes it as a new crate (the token needs `publish-new`), and `urna-runtime` stays at 0.5.3.

### Follow-up

- Before the next tag: the `CARGO_REGISTRY_TOKEN` must be allowed to publish `urna-engine` as a new crate.

## References

- ADR-0001, superseded by this record.
- Issue #296; sub-issues #391 to #395.
- `docs/CONTRIBUTING.md`, Naming.
