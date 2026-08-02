# security

`nest` is maintained by [hoff research](https://hoffresearch.com). author: brenner cruvinel.

## supported versions

only the latest minor on `main` is supported.

| version | status |
|---------|--------|
| 0.3.x   | supported (current) |
| 0.2.x   | not supported, please upgrade |
| 0.1.x   | not supported, please upgrade |

## reporting a vulnerability

do not open a public github issue for security vulnerabilities.

use one of:

- private vulnerability report: <https://github.com/hoffresearch/nest/security/advisories/new>
- email: brenner@hoffresearch.com

we aim to acknowledge within 72 hours and to publish a fix or mitigation within 14 days for confirmed reports. coordinated disclosure preferred; we credit reporters who request it.

## scope

things we treat as security bugs:

- malformed `.nest` files that trigger UB / OOB / panic in the rust runtime
- a citation collision (two distinct chunks producing the same `chunk_id`)
- a `content_hash` collision under the v1 hash domain separation
- a path that bypasses `model_hash` validation in `nest search-text` without the user passing `--skip-model-hash-check`
- secrets or credentials accidentally committed to the repository

things we do not treat as security bugs:

- low recall on a particular corpus
- HNSW recall under user expectation (configuration tuning, see `--ef`)
- BM25 tokenizer degrading on CJK / thai / lao (documented limitation, see `.contracts/.agents/AGENTS.md` known gaps)
- compressed vs raw size differences
- vulnerabilities in upstream sentence-transformers / huggingface stack; report those upstream first
- weaknesses in the embedding model itself (false positives, biased recall)
- configuration choices made by the operator (e.g. building a corpus with the placeholder `model_hash` and using `--skip-model-hash-check`)

## what helps a report

- the `.nest` `file_hash` and `content_hash` (`nest stats <file>` prints both)
- the runtime `simd_backend` and platform (`nest stats`)
- the exact CLI or python invocation
- a minimal reproducer if possible (a synthetic `.nest` is fine, see `crates/nest-format/tests/fixtures/`)
- whether you have a proposed mitigation

## hardening notes

- the runtime (rust) never opens a network socket. queries are answered from `mmap`. the default query embedders are offline too: `ask`/`retrieve` use the vendored potion table (no network by construction), and the `search-text` sentence-transformers path forces `HF_HUB_OFFLINE`/`TRANSFORMERS_OFFLINE` unless you opt in with `NEST_ALLOW_DOWNLOAD=1` (or pass `--model-path`).
- `model_hash` is a granular fingerprint over the local model snapshot (config + tokenizer + weights + pooling + dim + normalize). a mismatch fails with a typed error, never silently. the CLI (`search-text`) enforces this; the Python `NestFile.retrieve` binding accepts `expected_model_hash` and the flagship `forge/retrieve.py` passes it by default, so the honesty gate holds on the Python surface too.
- `unsafe` lives in the SIMD dispatcher (`crates/nest-runtime/src/simd/`), the mmap reader (`crates/nest-runtime/src/mmap_file.rs`), and a handful of zero-copy view casts in `crates/nest-format/src/layout/` and `encoding/int8.rs`. the SIMD and mmap sites carry `// SAFETY:` comments; documenting the remaining `nest-format` sites is a tracked hardening item (do not assume every block is annotated).
- untrusted `.nest` files: the header/section/footer checksums are unkeyed SHA-256 (corruption detection, NOT authenticity) — an attacker can recompute them, so `validate()` does not prove a file is trustworthy. Safety against a hostile file rests on the parser's memory-safety (bounds-checked indices, capped decompression/allocation); opening an untrusted corpus still executes that parser, so treat unknown `.nest` files with the same care as any untrusted input.
- release provenance: commits are signed (ssh signing), but release tags and `nest-cli` binaries are NOT yet cryptographically signed, and no SBOM is published per release. Treat a downloaded artifact as unverified against source until signed releases land. `Cargo.lock` is committed so the rust dependency set is pinned and auditable. This is a tracked hardening item.
