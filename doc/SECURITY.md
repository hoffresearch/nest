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
- a path that bypasses `model_hash` validation in a text query path (`search-text`, `ask`, `retrieve`) without the user passing an explicit skip flag; `search-space` takes a raw vector and validates only when `--expect-model-hash` is given
- a path that executes model-repo code (`trust_remote_code` presets) without the explicit opt-in — the spec's `allow_remote_code` at build time, `NEST_ALLOW_REMOTE_CODE` on the query/bench/bridge side (`embed_query_model.py`, `nest_model_bench.py`, `nest_ui_bridge.py`) — or with a code file whose sha256 is outside the pinned allowlist in `python/forge/model_registry.py`
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
- `unsafe` lives in the SIMD kernels (`crates/nest-runtime/src/simd/`) and the two `mmap` calls (`crates/nest-runtime/src/mmap_file.rs`, `mmap_cold.rs`); the former zero-copy casts in `crates/nest-format` (header / footer / section-entry byte views, the int8 row view) are now safe `bytemuck` casts whose layout invariants the compiler checks. every remaining `unsafe` block carries a `// SAFETY:` comment naming the invariant, and `clippy::undocumented_unsafe_blocks` is denied workspace-wide so a new undocumented block fails the build. the safe SIMD dispatchers check every slice length with `assert!` (kept in release), so the raw-pointer kernels never run on a mismatched row even if a validation layer upstream regresses.
- no `unwrap()` on a parse path: `clippy::unwrap_used` is denied workspace-wide (tests exempt), little-endian field reads go through `nest_format::bytes` and return `UnexpectedEof`, every header-derived size (`n * dim * width`) is overflow-checked, every payload cursor bounds-checks as `need > remaining` (never `pos + need > len`, which wraps), every count read from the file is bounded against the remaining bytes before it sizes an allocation, and every f32 ranking sort is a NaN-last total order.
- fuzzing: `cargo test` runs a deterministic mutation-fuzz harness on every push (`crates/nest-format/tests/mutation_fuzz.rs`, `crates/nest-runtime/tests/mutation_fuzz.rs`: bit flips, byte sets, zero runs, integer specials, truncation, splices, half of them with checksums resealed so the corruption reaches the decoders), and `fuzz/` carries four `cargo-fuzz` targets (reader, section codecs, index codecs, mmap open + every search verb) that `ci.yml` smoke-runs on nightly. the harnesses' first runs found and fixed five classes of bug: an unchecked `n * dim` overflow in the expected-section-size check, a NaN score reaching a `partial_cmp`-based sort (a panic since rust 1.81) through an unvalidated multimodal band, a wrapping `pos + n` cursor bounds check, BM25 postings whose doc ids were never checked against `n_docs`, and (from the coverage-guided run) a uri-pool count in the intpack spans repack that reached `Vec::with_capacity` unbounded, so a 90-byte payload asked the allocator for 31 GB. every count-driven allocation on a decode path is now bounded by what the bytes can hold before it happens. those are exactly the bug classes this document declares in scope; a `.nest` that panics or aborts the runtime is still a security bug, please report it.
- untrusted `.nest` files: the header/section/footer checksums are unkeyed SHA-256 (corruption detection, NOT authenticity); an attacker can recompute them, so `validate()` does not prove a file is trustworthy. Safety against a hostile file rests on the parser's memory-safety (bounds-checked indices, capped decompression/allocation); opening an untrusted corpus still executes that parser, so treat unknown `.nest` files with the same care as any untrusted input.
- release provenance: commits are signed (ssh signing) and every release artifact carries a per-file sha256 plus a sigstore keyless attestation (`gh attestation verify <artifact> --repo hoffresearch/nest`). release tags are not yet signed and no SBOM is published per release; those remain tracked hardening items. `Cargo.lock` is committed so the rust dependency set is pinned and auditable.
- model registry remote code: presets that require `trust_remote_code` (wemm, jina) load only with an explicit `allow_remote_code` opt-in in the build spec AND matching pinned sha256 allowlists for the model-repo code files. a hash identifies a version, it does not make it safe; review the pinned files before trusting a new pin, and prefer building in an isolated environment when the model directory is not fully trusted. sentence-transformers models run in a per-model worker process.
