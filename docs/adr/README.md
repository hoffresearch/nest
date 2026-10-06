---
project: urna
audience: users and contributors
status: active
last-updated: 2026-10-06
domain: decision-records
---

# Architecture decision records

An ADR records an architecture decision, or a lesson that must stay as reference, with its context and trade-offs. Operational lessons that only change how something is used belong in the doc that owns the topic (`docs/USAGE.md`, `docs/CONTRIBUTING.md`), not here. Current architecture lives in `docs/ATLAS.toml`; change history lives in `docs/CHANGELOG`.

## Writing one

1. Copy [TEMPLATE.md](TEMPLATE.md) to `docs/adr/<category>/NNNN-short-title.md`. `NNNN` is the next free number across all of `docs/adr/`, so ids stay unique; the title is kebab-case English.
2. Fill the YAML header: `id` matches the file number, `category` names the folder in capitals (`STRUCTURE` for `structure/`), `kind` is `decision` or `lesson`.
3. Keep it short: enough context to understand the choice, the options weighed and what the decision costs.
4. An accepted ADR is never deleted and its body stays as written. When a new ADR replaces it, the new one lists it in `supersedes`, and only the old one's header changes: `status: superseded`, `superseded-by` and `last-updated`.

## Categories

| Folder | Scope |
| --- | --- |
| [structure/](structure/) | System structure and component boundaries: crates, Rust and Python split, workspaces |
| [container/](container/) | The `.urna` file: what it can represent and how its bytes are laid out: layout, section and codec ids, hashes, integrity and compatibility |
| [interface/](interface/) | CLI, terminal UI, Python API and integration contracts |
| [embedders/](embedders/) | What defines and checks an embedding model's identity: what goes into `model_hash` and the fingerprint, the gate that refuses a mismatched model, query and build compatibility |
| [modelszoo/](modelszoo/) | Which models are offered and how they are obtained: the registry and catalog, presets, pinned revisions and snapshots, the install offer, the criteria to add or drop a model, sources and licenses |
| [packaging/](packaging/) | Dependencies, toolchain, platforms, packaging, distribution channels and publication |
| [retrieval/](retrieval/) | Indexes, kernels, SIMD, search routes, rerank and scoring |
| [safeguard/](safeguard/) | Offline guarantees, provenance, trust, consents and credentials |
| [workflows/](workflows/) | Tests, quality gates, CI and the development and documentation process |
| [datastore/](datastore/) | On-disk data outside the `.urna` file: source corpora, caches (embeddings, Hugging Face), build artifacts (build lock, sidecars), data roots, model and payload storage |
| [optimizer/](optimizer/) | Memory, mmap, caching, latency and resource limits across components |
| [stability/](stability/) | Recovery, partial failures, retries and degraded behavior |
| [encodings/](encodings/) | Which encoding and level to use and how quality loss is measured and gated: media codecs (AV1, AVIF, JXL, crf, GOP, the quality gate), embedding precision (int8, int4, the dtype ladder, MRL truncation) and the text codec choice |

A decision that spans categories goes in the folder of its main consequence, with the other categories as `tags`. A new area gets a folder and a row in this table in the same PR as its first ADR.

`EMBEDDERS` and `MODELSZOO` split on one question: if a decision changes what goes into `model_hash` or how it is checked, it belongs in `EMBEDDERS`; otherwise in `MODELSZOO`. A model of another kind (reranker, OCR, generator) is chosen in `MODELSZOO`, and how it is used goes in the folder of the area it serves. Consent to run remote code goes in `SAFEGUARD`, tagged `MODELSZOO`.

## Finding one

The YAML header makes records searchable without opening them:

```sh
rg -l '^category: RETRIEVAL' docs/adr  # every ADR in a category
rg -l '^status: accepted' docs/adr     # decisions in force
rg -l '^tags:.*mmap' docs/adr          # by tag
rg '^title:' docs/adr                  # one line per ADR
```
