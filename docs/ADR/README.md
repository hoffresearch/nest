---
project: urna
audience: users and contributors
status: active
last-updated: 2026-10-04
domain: decision-records
---

# Architecture decision records

An ADR records an architecture decision, or a lesson that must stay as reference, with its context and trade-offs. Operational lessons that only change how something is used belong in the doc that owns the topic (`docs/USAGE.md`, `docs/CONTRIBUTING.md`), not here. Current architecture lives in `docs/ARC.toml`; change history lives in `docs/CHANGELOG`.

## Writing one

1. Copy [TEMPLATE.md](TEMPLATE.md) to `docs/ADR/<category>/NNNN-short-title.md`. `NNNN` is the next free number across all of `docs/ADR/`, so ids stay unique; the title is kebab-case English.
2. Fill the YAML header: `id` matches the file number, `category` matches the folder, `kind` is `decision` or `lesson`.
3. Keep it short: enough context to understand the choice, the options weighed and what the decision costs.
4. An accepted ADR is never deleted and its body stays as written. When a new ADR replaces it, the new one lists it in `supersedes`, and only the old one's header changes: `status: superseded`, `superseded-by` and `last-updated`.

## Categories

| Folder | Scope |
| --- | --- |
| [architecture/](architecture/) | System structure and component boundaries: crates, Rust and Python split, workspaces |
| [format/](format/) | The `.urna` file: what it can represent and how its bytes are laid out: layout, section and codec ids, hashes, integrity and compatibility |
| [compression/](compression/) | Which encoding and level to use and how quality loss is measured and gated: media codecs (AV1, AVIF, JXL, crf, GOP, the quality gate), embedding precision (int8, int4, the dtype ladder, MRL truncation) and the text codec choice |
| [data/](data/) | Data lifecycle and persistence outside the file format: corpora, caches, build artifacts, model and payload storage |
| [search/](search/) | Indexes, kernels, SIMD, search routes, rerank and scoring |
| [models/](models/) | Embedders, model registry and catalog, model identity and fingerprints |
| [interfaces/](interfaces/) | CLI, terminal UI, Python API and integration contracts |
| [performance/](performance/) | Memory, mmap, caching, latency and resource limits across components |
| [reliability/](reliability/) | Recovery, partial failures, retries and degraded behavior |
| [security/](security/) | Offline guarantees, provenance, trust, consents and credentials |
| [release/](release/) | Dependencies, toolchain, platforms, packaging, distribution channels and publication |
| [workflow/](workflow/) | Tests, quality gates, CI and the development and documentation process |

A decision that spans categories goes in the folder of its main consequence, with the other categories as `tags`. A new area gets a folder and a row in this table in the same PR as its first ADR.

## Finding one

The YAML header makes records searchable without opening them:

```sh
rg -l '^category: search' docs/ADR      # every ADR in a category
rg -l '^status: accepted' docs/ADR      # decisions in force
rg -l '^tags:.*mmap' docs/ADR           # by tag
rg '^title:' docs/ADR                   # one line per ADR
```
