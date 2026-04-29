# security

`nest` is maintained by [hoff research](https://hoffresearch.com). author: brenner cruvinel.

## supported versions

only the latest minor on `main` is supported.

| version | status |
|---------|--------|
| 0.2.x   | supported |
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
- BM25 tokenizer degrading on CJK / thai / lao (documented limitation, see `AGENTS.md` known gaps)
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

- the runtime never opens a network socket. queries are answered from `mmap`.
- `model_hash` is a granular fingerprint over the local model snapshot (config + tokenizer + weights + pooling + dim + normalize). a mismatch fails with a typed error, never silently. see `kdb/adr/0008`.
- `unsafe` is concentrated in the SIMD dispatcher (`crates/nest-runtime/src/simd/`) and the mmap reader (`crates/nest-runtime/src/mmap_file.rs`). every `unsafe` block carries a `// SAFETY:` comment documenting the invariant.
- binary releases of `nest-cli` are not yet signed. signed tags are on the v0.3 backlog.
