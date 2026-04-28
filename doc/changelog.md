# changelog

all notable changes to `nest` are documented here.

format follows [keep a changelog](https://keepachangelog.com/en/1.1.0/). versioning follows [semver](https://semver.org/spec/v2.0.0.html). the on-disk container format is frozen at v1; breaking changes bump `NEST_FORMAT_VERSION` (binary) or `NEST_SCHEMA_VERSION` (manifest fields).

## [0.2.0] - 2026-04-28

production-ready release. extends v1 with new section encodings, optional ANN and lexical sections, runtime SIMD dispatch, and offline model verification. existing v0.1 files load unchanged in v0.2 readers.

### added

section encodings within v1 (no format-version bump):

- `encoding=1` zstd for text-heavy sections (`chunks_canonical`, `chunks_original_spans`, `provenance`, `search_contract`, `bm25_index`). reader decodes transparently. embeddings are never zstd-compressed.
- `encoding=2` float16 embeddings. writer converts f32 to f16; runtime decodes lane-by-lane and accumulates in f32.
- `encoding=3` int8 embeddings. per-vector f32 scale plus n*dim i8 quantized values. always paired with rerank against an exact path or HNSW.

optional sections (skipped by older readers, not part of `content_hash`):

- `0x07 hnsw_index`. pure-rust HNSW (Malkov-Yashunin Algorithm 4 heuristic). build is deterministic given a seed. search returns candidates; runtime always reranks with the exact dot product so the final score is real cosine.
- `0x08 bm25_index`. inverted index over tokenized chunk text. used by hybrid search via reciprocal-rank fusion (RRF, k=60) with the vector path, then exact rerank on the union.

CLI:

- `nest search-ann <file> <qvec> -k K --ef N`: force the HNSW path with explicit `ef_search`.
- `nest search-text <file> "query" -k K [--model-path PATH] [--skip-model-hash-check]`: shells out to `python/embed_query.py`, validates the embedder's `embedding_model`, `embedding_dim`, and `model_hash` against the manifest, then routes to the declared `index_type` (exact, hnsw, hybrid). a model mismatch fails with a typed error, never silently. supersedes ADR 0005.
- `nest benchmark --madvise-cold`: extra benchmark pass calling `posix_madvise(MADV_DONTNEED)` between queries. upper bound on cold-cache latency, not absolute cold (see `MmapNestFile::madvise_cold` for caveats).
- `nest inspect --json`: structured output mirroring `MmapNestFile::inspect_json` for programmatic consumers.

build presets:

| preset       | text encoding | embeddings | ANN | BM25 | size_ratio | recall@10 |
|--------------|---------------|------------|-----|------|-----------:|----------:|
| `exact`      | raw           | float32    | no  | no   |      1.000 |    1.0000 |
| `compressed` | zstd          | float16    | no  | no   |      0.350 |    1.0000 |
| `tiny`       | zstd          | int8       | yes | no   |      0.283 |    0.9920 |
| `hybrid`     | zstd          | float32    | yes | yes  |      0.668 |    1.0000 |

measured on the project's PT-BR fake-news corpus (n=30,725, dim=384, NEON, 100 queries, k=10).

SIMD dispatch:

- per-dtype dot-product backends: AVX2 on x86_64, NEON on aarch64, scalar fallback.
- detection at runtime via `is_x86_feature_detected!` / `is_aarch64_feature_detected!`.
- `NEST_FORCE_SCALAR=1` forces the scalar fallback for A/B benchmarks.
- accumulators always in f32 regardless of dtype.

model fingerprint:

- `python/model_fingerprint.py`: reproducible model fingerprint composed of `{model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings}`. JCS-canonical JSON, hashed to produce the manifest's `model_hash`.
- builder refuses to write the legacy zero-placeholder (`sha256:0...0`).
- `python/embed_query.py` emits structured JSON: `{model_hash, fingerprint, embedding_model, embedding_dim, vector}`.
- CLI accepts `--model-path /path/to/snapshot` for fully-offline operation.

manifest:

- `dtype` extended to `"float32" | "float16" | "int8"`.
- `index_type` extended to `"exact" | "hnsw" | "hybrid"`.
- `rerank_policy` extended to `"none" | "exact"`.
- `capabilities.supports_ann` and `capabilities.supports_bm25` reflect the optional sections actually present.

tooling:

- `python/tools/measure_presets.py`: builds all 4 presets from a baseline `.nest`, measures size / recall / score drift / p50/p95/p99 latency. emits markdown table or `--json` for regression gates.
- `python/tools/compare_measure.py`: validates two `--json` dumps against 6 production gates (size ratios, recall floors, p95 headroom). non-zero exit on any failure.
- `scripts/release_check.sh`: end-to-end CI gate (build, test, clippy, fmt, line-count guard, python tests, ruff, measure, compare). single source of truth for "PR-ready".

infrastructure:

- HNSW recall fix: replaced naive `top-m` neighbor selection with `select_neighbors_heuristic` (Malkov-Yashunin Algorithm 4). bumped `DEFAULT_EF_CONSTRUCTION` to 400 to hit recall@10 >= 0.95 at typical corpus sizes.
- file hygiene cap: every rust source file in `crates/**/src/**` and every first-party python module is at most 300 lines (see `kdb/adr/0011`). test files exempt.
- ADRs added: 0006 (encodings 1/2/3), 0007 (HNSW + BM25 optional sections), 0008 (granular model_hash fingerprint), 0009 (runtime SIMD dispatch), 0010 (search-text supersedes 0005), 0011 (file hygiene 300-line rule).
- `kdb/` removed from `.gitignore`. earlier sessions had it gitignored, silently dropping every architectural decision committed there.

### test surface

- 134 rust tests (was 70 in v0.1):
  - 47 `nest-format` unit + roundtrip tests
  - new: `dual_integrity.rs` (3 cases: encoding-invariant content_hash, mismatched section/file hashes)
  - new: `negative_zstd.rs` (3 cases: bad encoding values, embedding zstd refusal, encoding-mismatch)
  - new: `negative_fp16.rs` (4 cases: NaN, Inf, odd dim SIMD parity)
  - new: `negative_int8.rs` (4 cases: bad payload version, bad scale_kind, NaN scale, truncation)
  - new: `v01_compat.rs` (1 case: golden v0.1 fixture loads in v0.2 reader byte-identical)
  - new: `hnsw_recall.rs` (3 cases: recall@10 >= 0.95, recall@1 >= 0.90, realistic-size sanity)
  - new: `fp16_topk_recall_vs_f32.rs` (1 case: recall@10 >= 0.98 vs f32, drift <= 1e-4)
- python: 3 test scripts (`test_e2e.py`, `test_builder.py`, `test_search_text_model_hash.py` with 5 cases).
- `cargo fmt --all --check` clean. `cargo clippy --workspace --all-targets -- -D warnings` clean. `ruff check . && ruff format --check .` clean.

### compatibility

v0.1 files (raw + float32 + 6 required sections) load unchanged in v0.2 readers. the byte-frozen golden fixture at `crates/nest-format/tests/fixtures/golden_v1_minimal.nest` is verified every CI run.

v0.2 files that use only `encoding=raw` and `dtype=float32` still load in v0.1 readers, with one caveat: optional sections 0x07 and 0x08 are unknown to v0.1 readers and get skipped (file still loads). v0.1 readers reject `encoding ∈ {1,2,3}` and `dtype ∈ {"float16", "int8"}` with `UnsupportedSectionEncoding` or `UnsupportedDType`, never silently.

[0.2.0]: https://github.com/hoffresearch/nest/releases/tag/v0.2.0

## [0.1.0] - 2026-04-27

first public release. the on-disk format, hash semantics, citation URI, manifest contract, and CLI surface listed below are frozen for v1: any change must bump `NEST_FORMAT_VERSION` (binary container) or `NEST_SCHEMA_VERSION` (manifest fields).

### frozen binary container

- file magic: `NEST` (`0x4E 0x45 0x53 0x54`).
- `NEST_VERSION_MAJOR = 1`, `NEST_VERSION_MINOR = 0`, `NEST_FORMAT_VERSION = 1`, `NEST_SCHEMA_VERSION = 1`.
- header: 128 bytes, `repr(C)`, compile-time asserted. fields (LE, unsigned): `magic`, `version_major/minor`, `flags`, `embedding_dim`, `n_chunks`, `n_embeddings`, `file_size`, `section_table_offset`, `section_table_count`, `manifest_offset`, `manifest_size`, `header_checksum[8]`, `reserved[48]`.
- section table entry: 32 bytes, `repr(C)`, compile-time asserted. fields: `section_id(u32)`, `encoding(u32)`, `offset(u64)`, `size(u64)`, `checksum[8]`.
- footer: 40 bytes (`u64 footer_size = 40`, `[u8; 32] file_hash`).
- section payload alignment: every section's `offset` is a multiple of `SECTION_ALIGNMENT = 64`. padding between sections is zero and excluded from each section's checksum (but covered by the footer hash).
- endianness: little-endian, unsigned unless explicitly noted.

### required sections (canonical, alphabetical for `content_hash`)

| ID     | Name                       |
| ------ | -------------------------- |
| `0x01` | `chunk_ids`                |
| `0x02` | `chunks_canonical`         |
| `0x03` | `chunks_original_spans`    |
| `0x04` | `embeddings`               |
| `0x05` | `provenance`               |
| `0x06` | `search_contract`          |

encoding: only `SECTION_ENCODING_RAW = 0` is accepted in v0.1. values `1 = zstd`, `2 = float16`, `3 = int8` are reserved (and shipped in v0.2).

### hashing

- primary hash: SHA-256 throughout. no BLAKE3.
- `header_checksum`: first 8 bytes of `SHA-256(header[0..72] ++ header[80..128])`. header with its own checksum slot zeroed.
- section `checksum`: first 8 bytes of `SHA-256(payload)`. padding is not hashed.
- `file_hash` (footer): full 32-byte `SHA-256(file[0..file_size-40))`, including padding.
- `content_hash`: 32-byte `SHA-256` over the canonical sections in alphabetical-by-name order, each domain-separated by length-prefixed name and length-prefixed payload. stable across rebuilds of the same content.
- `chunk_id`: domain-separated `SHA-256` with literal preimage prefix `"nest:chunk_id:v1\n"`. format `sha256:<64 hex chars>`.
- `model_hash`: caller-supplied; format `sha256:<64 hex chars>` enforced at write time. v0.1 accepted any value matching the regex; v0.2 enforces a real fingerprint (see ADR 0008).

### manifest contract

JCS-style canonical JSON (declaration-ordered known fields, BTreeMap order for `extra`, no whitespace). required values for v0.1:

- `dtype = "float32"`, `metric = "ip"`, `score_type = "cosine"`, `normalize = "l2"`, `index_type = "exact"`, `rerank_policy = "none"`.
- `capabilities.supports_exact = true`, `capabilities.supports_reproducible_build = true`.
- `model_hash` matches `sha256:<64 hex>` regex.

### reproducibility

- `NestFileBuilder::reproducible(true)` overrides `manifest.created` to `"1970-01-01T00:00:00Z"` (`REPRODUCIBLE_CREATED`).
- two builds with identical inputs produce byte-identical files. verified on the legacy converter (`data/truw_ptbr.nest` to 73.73 MB v0.1 binary); both builds shasum to `b9f6e0ea16176706f08767559927737ce91070147ec6cb54e26710bff3d2566d`.

### version skew policy

- reader rejects `format_version` or `schema_version` greater than its own constants (`NestError::UnsupportedFormatVersion` / `UnsupportedSchemaVersion`).
- reader accepts equal or smaller versions.
- header version: `version_major != 1` rejected; `version_minor` may drift downward.

### CLI v0.1 surface

frozen subcommands in `nest-cli`:

| Command           | Behavior                                                           |
| ----------------- | ------------------------------------------------------------------ |
| `nest inspect`    | header, section table, manifest, hashes                            |
| `nest validate`   | full integrity check (header / sections / footer / manifest)        |
| `nest stats`      | size, chunk count, dim, model, hashes, per-section sizes            |
| `nest search`     | exact top-k search; query is a JSON array of f32                    |
| `nest benchmark`  | latency stats over N random queries                                |
| `nest cite`       | resolve `nest://content_hash/chunk_id` to `(text, span, hashes)`    |

the v0.1 CLI does not ship an embedding model. text to vector is the caller's responsibility (see `python/builder.py` and `python/convert_legacy.py` for examples using sentence-transformers). v0.2 added `nest search-text` to fill this gap.

### citation URI

`nest://<content_hash>/<chunk_id>` where both halves are full `sha256:<hex>` strings. `nest cite` rejects citations whose `content_hash` does not match the file's `content_hash`.

### search contract

`SearchHit` exposes: `chunk_id`, `score` (real f32 cosine in `[-1, 1]`), `score_type = "cosine"`, `source_uri`, `offset_start`, `offset_end`, `embedding_model`, `index_type = "exact"`, `reranked = false`, `file_hash`, `content_hash`, `citation_id`. top-k uses a stable sort by score descending, breaking ties by index ascending. `recall = 1.0` always; `truncated = (k < n_embeddings)`.

### test surface

- 70 rust tests (`cargo test --workspace`):
  - 34 `nest-format` unit tests (layout, manifest, sections, chunk, writer)
  - 5 `nest-format` golden-fixture tests (1366-byte minimal `.nest`)
  - 19 `nest-format` roundtrip / negative tests (truncation, magic, encoding, alignment, version skew, dim mismatch, NaN/Inf)
  - 8 `nest-runtime` flat-search tests
  - 4 `nest-cli` integration tests (`validate`, `search`, `inspect`, `cite`)
- 4 python tests (`tests/test_e2e.py`, PyO3-only) and 3 builder tests (`tests/test_builder.py`).
- `cargo fmt --all --check` clean; `cargo clippy --workspace -- -D warnings` clean.

### python bindings

`python/_nest.so` (PyO3, abi3-py312). wrapper `python/nest.py` exposes:

- `nest.open(path) -> NestFile`
- `NestFile.search(qvec, k) -> [SearchHit]`
- `NestFile.inspect()` / `NestFile.validate()`
- `NestFile.embedding_dim` / `n_embeddings` / `file_hash` / `content_hash`
- `nest.build(...)` (writer glue)
- `nest.chunk_id(...)` (deterministic id derivation)

single python entry point: no subprocess CLI fallback inside `python/`.

### reference artefacts

- golden fixture: `crates/nest-format/tests/fixtures/golden_v1_minimal.nest` (1366 bytes; regenerate with `cargo run -p nest-format --example regen_golden`).
- legacy SQLite-based dataset: `data/truw_ptbr.nest` (28 MB) to `data/truw_ptbr.v1.nest` (73.73 MB, 19,769 chunks dim 384) via `python/convert_legacy.py`.
- specification: `doc/spec.md`.

[0.1.0]: https://github.com/hoffresearch/nest/releases/tag/v0.1.0
