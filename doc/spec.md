# `.nest` v1 — Binary Format Specification

`.nest` v1 is a self-contained, hash-verified, mmap-friendly container for a
semantic knowledge base: canonical chunks, their embeddings, their original
spans, and a runtime contract that pins every search-time decision.

This document is the byte-by-byte source of truth for v1. The Rust
implementation in `crates/nest-format` is the canonical encoder/decoder; this
spec is what every other implementation must agree with.

The format is **stable at v1**: `NEST_FORMAT_VERSION` is `1` and any change
that alters the header/footer/section-table layout bumps it. Within v1, new
**section encodings** (zstd / float16 / int8) and new **optional sections**
(HNSW, BM25) extend the format without breaking older files. Manifest-only
schema additions bump `NEST_SCHEMA_VERSION`.

**Backward / forward compatibility within v1:**

- Old files (raw + float32 + 6 required sections) load unchanged in any v1
  reader — the golden fixture at `tests/fixtures/golden_v1_minimal.nest` is
  byte-frozen and tested every CI run.
- New files that use only encoding=raw and dtype=float32 still load in older
  v1 readers.
- New files that use encoding ∈ {1,2,3} or include optional sections get
  rejected by older v1 readers with `UnsupportedSectionEncoding` /
  `UnsupportedDType` — explicit, typed, never silent.

---

## 1. Principles

- **Reader-first.** The reader rejects any malformed file with a typed
  error. No "best effort" parsing.
- **Hash everything, twice.** Section checksums hash the *physical* bytes
  on disk (catch bit-flip / disk corruption). `content_hash` hashes the
  *decoded* bytes (catch semantic drift across encodings). `file_hash`
  covers the whole physical artefact including padding. Three guarantees,
  three failure modes.
- **Recall before throughput.** Exact flat search is always available and
  is the recall=1.0 ground truth. ANN (HNSW) and hybrid (BM25 ∪ vector)
  paths exist as opt-ins; both **always** rerank candidates with the
  exact dot product so the final score is the real cosine value.
- **Reproducible by construction.** Two builds with identical inputs and
  identical encoding choices produce byte-identical output when
  `--reproducible` is set. Includes the HNSW graph (deterministic seed)
  and the BM25 index (alphabetical term order).
- **Citable.** Every chunk has a stable `chunk_id`; every hit is
  resolvable via `nest://<content_hash>/<chunk_id>` against the file's
  own hash. Citations are stable across wire encodings (zstd vs raw)
  because `content_hash` is over the *decoded* canonical sections.
- **mmap-friendly.** Embeddings sections are never zstd-compressed —
  they live in mmap and the SIMD dot product reads them in place.

## 2. Primitives

- **Endianness.** All multi-byte integers are little-endian, unsigned
  unless explicitly marked otherwise. There are no signed integers in
  the v1 wire format.
- **Strings.** Length-prefixed UTF-8: `u32 LE length` then `length`
  bytes. No NUL terminators.
- **Hash.** SHA-256 is the only hash function used. Fields named
  `*_hash` and `*_checksum` are SHA-256 outputs (or truncations thereof,
  always documented). BLAKE3 is not used in v1.
- **Hex.** Hex strings are lowercase. The format `sha256:<64 hex chars>`
  is used wherever a hash is exposed as text.
- **Alignment.** Every section payload starts at a 64-byte aligned
  offset (`SECTION_ALIGNMENT = 64`). The bytes between the previous
  section's end and the next section's start are padding, are filled
  with zero, and are **not** part of any section's checksum.

## 3. File layout

```text
+----------------------------------------------------------------------+
| [0   .. 128)        NestHeader                                        |
| [128 .. 128+T*32)   SectionTable, T = section_table_count entries     |
| [M   .. M+S)        Manifest (JCS canonical JSON, no whitespace)      |
| ... aligned to 64 ...                                                 |
| [O0  .. O0+L0)      section[0] data       \                           |
| ... aligned to 64 ...                       \                         |
| [O1  .. O1+L1)      section[1] data          | each O_i % 64 == 0     |
| ...                                          /                        |
| [On-1.. On-1+Ln-1)  section[n-1] data     /                           |
| [F   .. F+40)       Footer                                            |
+----------------------------------------------------------------------+
F = file_size - 40
```

Layout invariants:

- `section_table_offset == 128`.
- `manifest_offset == 128 + section_table_count * 32`.
- For every entry `i` in the table, `entry.offset % 64 == 0`.
- The first section starts at `align_up_64(manifest_offset + manifest_size)`.
- Section `i+1` starts at `align_up_64(section[i].offset + section[i].size)`.
- The footer starts at the unaligned end of the last section, i.e.
  `last.offset + last.size`.
- `file_size == footer_offset + 40`.

## 4. Header — 128 bytes (`NestHeader`)

| Offset | Size | Field                  | Value / semantics                                  |
| ------ | ---- | ---------------------- | -------------------------------------------------- |
| 0      | 4    | `magic`                | ASCII `"NEST"` (`0x4E 0x45 0x53 0x54`)             |
| 4      | 2    | `version_major`        | `1`                                                |
| 6      | 2    | `version_minor`        | `0`                                                |
| 8      | 4    | `flags`                | reserved, MUST be `0` in v1                        |
| 12     | 4    | `embedding_dim`        | embedding dimensionality, > 0                      |
| 16     | 8    | `n_chunks`             | number of chunks                                   |
| 24     | 8    | `n_embeddings`         | always equal to `n_chunks` in v1                   |
| 32     | 8    | `file_size`            | total file size in bytes (must equal actual size)  |
| 40     | 8    | `section_table_offset` | always `128`                                       |
| 48     | 8    | `section_table_count`  | number of section table entries                    |
| 56     | 8    | `manifest_offset`      | byte offset of the manifest                        |
| 64     | 8    | `manifest_size`        | manifest length in bytes                           |
| 72     | 8    | `header_checksum`      | first 8 bytes of `SHA-256(header[0..72] ++ header[80..128])` |
| 80     | 48   | `reserved`             | MUST be all zero in v1                             |

The header checksum hashes the header with its own checksum slot zeroed
out, so a reader can verify the header in place before trusting any of
its offsets.

## 5. Section table entry — 32 bytes (`SectionEntry`)

| Offset | Size | Field        | Semantics                                            |
| ------ | ---- | ------------ | ---------------------------------------------------- |
| 0      | 4    | `section_id` | one of the section IDs in §6                         |
| 4      | 4    | `encoding`   | wire encoding (0/1/2/3 — see below)                  |
| 8      | 8    | `offset`     | byte offset of the payload, MUST satisfy `offset % 64 == 0` |
| 16     | 8    | `size`       | payload length in bytes (excludes padding)           |
| 24     | 8    | `checksum`   | first 8 bytes of `SHA-256(file[offset..offset+size])` — over the **physical** bytes as stored |

**Encoding values:**

| Code | Name      | Applies to                              | Decoded payload                                    |
| ---- | --------- | --------------------------------------- | -------------------------------------------------- |
| `0`  | `raw`     | any section                             | the bytes are the canonical payload                |
| `1`  | `zstd`    | every section **except** `embeddings`   | zstd-compressed canonical payload; reader decompresses transparently |
| `2`  | `float16` | only `embeddings` (`0x04`)              | `n * dim * 2` bytes of IEEE binary16 LE; manifest MUST declare `dtype = "float16"` |
| `3`  | `int8`    | only `embeddings` (`0x04`)              | quantized layout (see §6.5.2); manifest MUST declare `dtype = "int8"` |

Encodings 1, 2, 3 are extensions added within v1. A reader that does not
implement them MUST reject the file with `UnsupportedSectionEncoding` —
never silently fall back. Embeddings are never zstd-compressed because
SIMD reads them straight from mmap.

The section `checksum` always hashes the **physical** stored bytes
(compressed bytes for zstd, quantized bytes for int8/float16). This
catches disk corruption regardless of encoding. `content_hash` (§13)
catches semantic divergence across encodings.

## 6. Sections

Every v1 file MUST contain the following six **required** sections, in
any order in the file but with strictly increasing `section_id`s in the
section table (the writer sorts by `section_id`).

| ID     | Name (canonical)         | Payload                                           |
| ------ | ------------------------ | ------------------------------------------------- |
| `0x01` | `chunk_ids`              | length-prefixed UTF-8 strings, count == `n_chunks` |
| `0x02` | `chunks_canonical`       | length-prefixed UTF-8 strings, count == `n_chunks` |
| `0x03` | `chunks_original_spans`  | per-chunk `(source_uri, byte_start, byte_end)`     |
| `0x04` | `embeddings`             | dtype-specific (see §6.5)                          |
| `0x05` | `provenance`             | versioned JSON blob (free-form, deterministic)     |
| `0x06` | `search_contract`        | versioned JSON document mirroring the manifest     |

A reader MUST reject a file that omits any of the six with
`MissingRequiredSection(name)`. The set is closed: `content_hash` (§13)
is defined over these six names sorted alphabetically.

Two **optional** sections may be present when the corresponding
capability is declared in the manifest:

| ID     | Name (canonical)  | Required when                                          | Payload         |
| ------ | ----------------- | ------------------------------------------------------ | --------------- |
| `0x07` | `hnsw_index`      | `manifest.index_type ∈ {"hnsw", "hybrid"}` AND `capabilities.supports_ann = true` | see §6.8 |
| `0x08` | `bm25_index`      | `manifest.index_type = "hybrid"` AND `capabilities.supports_bm25 = true`          | see §6.9 |

Optional sections are **not** included in `content_hash` so adding an
HNSW or BM25 index to an existing corpus does not invalidate citations.

### 6.1 Section payload prefix

Every section except `embeddings` (`0x04`) begins with a 12-byte
prefix:

| Offset | Size | Field             | Value          |
| ------ | ---- | ----------------- | -------------- |
| 0      | 4    | `payload_version` | `1` in v1      |
| 4      | 8    | `count` / `length`| section-specific (see below) |

A reader rejects any unknown `payload_version` with
`UnsupportedSectionVersion`.

### 6.2 `chunk_ids` (`0x01`)

```text
prefix.payload_version = 1
prefix.count           = n_chunks
for i in 0..n_chunks:
    u32 LE  len_i
    bytes   chunk_id_i (UTF-8, len_i bytes, "sha256:<64 hex>")
```

`chunk_id_i` is derived deterministically from chunk content (§12).
Every entry MUST start with `"sha256:"` followed by 64 lowercase hex
characters.

### 6.3 `chunks_canonical` (`0x02`)

```text
prefix.payload_version = 1
prefix.count           = n_chunks
for i in 0..n_chunks:
    u32 LE  len_i
    bytes   canonical_text_i (UTF-8)
```

### 6.4 `chunks_original_spans` (`0x03`)

```text
prefix.payload_version = 1
prefix.count           = n_chunks
for i in 0..n_chunks:
    u32 LE  len_i
    bytes   source_uri_i (UTF-8)
    u64 LE  byte_start_i
    u64 LE  byte_end_i        (byte_end >= byte_start)
```

`byte_start` and `byte_end` index into the UTF-8 bytes of the original
source identified by `source_uri_i`. They are NOT character offsets and
are NOT relative to `canonical_text_i`. The relationship between
canonical text and original text (whitespace normalization, casing,
etc.) is the chunker's responsibility — `canonical_text` is what the
embedding was computed over, the span points back to the source for
provenance.

### 6.5 `embeddings` (`0x04`)

The payload layout depends on `manifest.dtype` and on the section's
`encoding` (which MUST agree — see §5):

#### 6.5.1 `dtype = "float32"`, `encoding = 0` (raw)

No prefix. The payload is exactly `n_embeddings * embedding_dim * 4`
bytes of `float32 LE`. Element ordering matches `chunk_ids` and
`chunks_canonical`: `embeddings[i]` is the embedding for `chunk_id[i]`.

#### 6.5.2 `dtype = "float16"`, `encoding = 2`

No prefix. The payload is exactly `n_embeddings * embedding_dim * 2`
bytes of IEEE binary16 LE (half precision). Same row-major ordering as
float32. The runtime always accumulates the dot product in `f32`; the
query stays `f32`. Lossy: typical score drift vs float32 < 5e-5,
recall@10 = 1.0 in practice on 384-dim corpora.

#### 6.5.3 `dtype = "int8"`, `encoding = 3`

Per-vector quantization with f32 scale. Layout:

```text
u32 LE   payload_version = 1
u32 LE   scale_kind      = 0   (per-vector f32; only kind defined in v1)
f32 LE   * n              one scale per vector
i8       * (n * dim)      quantized vectors, row-major
```

Where `f32_value ≈ i8_value * scale`. The quantizer maps `max(|v|)` of
each vector to ±127 to use the full int8 range. Search accumulates in
`f32`. Lossy: score drift ≈ 1e-3, recall@10 ≈ 0.95-0.98 depending on
corpus.

#### 6.5.4 Common to all dtypes

- Vectors MUST be L2-normalized at build time (the manifest declares
  `normalize = "l2"`) so the dot product equals real cosine.
- For float32 / float16: no `NaN`, no `Inf`. `validate_embeddings_values`
  walks the bytes and rejects either.
- For int8: the per-vector scales MUST be finite; the i8 bytes cannot
  encode NaN / Inf.

### 6.6 `provenance` (`0x05`)

```text
u32 LE  payload_version = 1
u64 LE  json_len
bytes   json_payload (UTF-8 JSON)
```

The JSON is free-form so the format does not constrain ingestion
metadata, but it MUST round-trip through `serde_json` (so it must be
valid JSON). Reasonable conventions:

- `legacy_source` / `legacy_version` — origin description.
- `retrieved_at` — ISO-8601 timestamp the source was crawled.
- `license` — SPDX identifier or free text (the manifest also carries a
  `license` field for the dataset as a whole; per-source licenses go
  here).
- `labels`, `sources` — per-chunk auxiliary metadata.

### 6.7 `search_contract` (`0x06`)

```text
u32 LE  payload_version = 1
u64 LE  json_len
bytes   json_payload (UTF-8 JSON)
```

The JSON MUST conform to:

```json
{
  "metric":         "ip",
  "score_type":     "cosine" | "hybrid_rrf",
  "normalize":      "l2",
  "index_type":     "exact" | "hnsw" | "hybrid",
  "rerank_policy":  "none"   | "exact"
}
```

Constraints (also enforced on the manifest):

- `index_type ∈ {"hnsw", "hybrid"}` REQUIRES `rerank_policy = "exact"`.
  ANN candidates always rerank into the exact dot product so the
  returned `score` is real cosine.
- `index_type = "hybrid"` REQUIRES `score_type = "hybrid_rrf"` (the
  candidate fusion uses reciprocal-rank fusion before exact rerank).

The reader cross-checks this with the manifest at open time; any
disagreement is rejected with the relevant `Unsupported*` error.

### 6.8 `hnsw_index` (`0x07`, optional)

Pure-Rust HNSW (Malkov & Yashunin, 2018). Always `encoding = 0` (raw)
because the graph is binary and mostly random. Layout:

```text
u32 LE  payload_version = 1
u32 LE  m                   — neighbor count at non-zero layers
u32 LE  m_max0              — neighbor count at layer 0 (typically 2*m)
u32 LE  ef_construction
u32 LE  entry_point         — node id of the entry vertex
u32 LE  max_level           — highest layer with any node (0-based)
u32 LE  n_nodes             — equal to header.n_embeddings
for each node i in 0..n_nodes:
    u32 LE  level_i
    for layer in 0..=level_i:
        u32 LE  k_i_l       — neighbor count at this layer
        u32 LE * k_i_l      — neighbor ids
```

Construction is deterministic given the same input vectors and the
same seed. Search returns a candidate set; the runtime reranks with
the exact dot product against the `embeddings` section so the final
score is the real cosine.

### 6.9 `bm25_index` (`0x08`, optional)

Inverted index for the lexical leg of hybrid search. Tokenization is
Unicode-aware (`char.is_alphanumeric()`), lowercased, drops tokens
shorter than 2 chars. Layout:

```text
u32 LE  payload_version = 1
f32 LE  k1                  — BM25 hyperparameter (default 1.5)
f32 LE  b                   — BM25 hyperparameter (default 0.75)
f32 LE  avgdl               — average doc length in tokens
u32 LE  n_docs
u32 LE  n_terms
for d in 0..n_docs:
    u32 LE  dl_d            — doc length in tokens
for t in 0..n_terms (alphabetical):
    u32 LE  len_t
    bytes   token_t         — UTF-8
    u32 LE  df_t            — document frequency
    for posting in postings_t (sorted by doc id):
        u32 LE  doc_id
        u32 LE  tf
```

Reproducible by construction (terms sorted alphabetically, postings
by doc id). Hybrid search fuses BM25 candidates with the vector
candidates via reciprocal-rank fusion (RRF, k=60), then exact-reranks
the union with cosine.

## 7. Footer — 40 bytes (`NestFooter`)

| Offset | Size | Field        | Semantics                                       |
| ------ | ---- | ------------ | ----------------------------------------------- |
| 0      | 8    | `footer_size`| `40`                                             |
| 8      | 32   | `file_hash`  | `SHA-256(file[0..file_size-40])` (32 bytes)      |

`file_hash` covers everything before the footer, **including** padding.
A bit-flip anywhere in the file (header, section table, manifest, any
section payload, or any padding byte) produces a footer hash mismatch.

## 8. Hashing summary

Three integrity guarantees, three different scopes:

| Name              | Algorithm | Width | Coverage                                       | Catches                |
| ----------------- | --------- | ----- | ---------------------------------------------- | ---------------------- |
| `header_checksum` | SHA-256   | 8 B   | header bytes `[0..72) ++ [80..128)`            | header tampering       |
| section `checksum`| SHA-256   | 8 B   | `file[offset..offset+size]` — **physical** stored bytes | bit-flip / disk corruption regardless of encoding |
| `file_hash` (footer) | SHA-256 | 32 B | `file[0..file_size-40)`, padding included     | any byte change in the artefact |
| `content_hash`    | SHA-256   | 32 B  | canonical sections in alphabetical order, **decoded** payload (§13) | semantic divergence; same content_hash across raw vs zstd of identical content |
| `chunk_id`        | SHA-256   | 32 B  | domain-separated preimage (§12)                | chunk-level dedup / citation |
| `model_hash`      | SHA-256   | 32 B  | caller-supplied; format `sha256:<64 hex>`      | model identity         |

Two-checksum design:

- A bit-flip inside a zstd-compressed section payload trips the section
  `checksum` (over physical bytes) — caught before decompression even
  runs, so we never feed garbage to the decompressor.
- Two files with the same logical content but different wire encodings
  (raw vs zstd of the same chunks_canonical, etc.) share the same
  `content_hash` and therefore the same citation URIs.
- Two files with identical `content_hash` MAY have different `file_hash`
  (different wire encoding, different padding, different optional
  sections present).

## 9. Manifest

The manifest is a single JCS-style canonical JSON document encoded in
UTF-8 with no whitespace. Field order follows declaration order; the
free-form `extra` map is serialized in `BTreeMap` order so it is
deterministic.

| Field                  | Type                        | Required | v1 value / constraint                              |
| ---------------------- | --------------------------- | :------: | -------------------------------------------------- |
| `format_version`       | `u32`                       |  yes     | `1`                                                |
| `schema_version`       | `u32`                       |  yes     | `1`                                                |
| `embedding_model`      | `string`                    |  yes     | non-empty                                          |
| `embedding_dim`        | `u32`                       |  yes     | `> 0`, equal to `header.embedding_dim`             |
| `n_chunks`             | `u64`                       |  yes     | `> 0`, equal to `header.n_chunks`                  |
| `dtype`                | `string`                    |  yes     | `"float32"` \| `"float16"` \| `"int8"`             |
| `metric`               | `string`                    |  yes     | `"ip"`                                             |
| `score_type`           | `string`                    |  yes     | `"cosine"` \| `"hybrid_rrf"`                       |
| `normalize`            | `string`                    |  yes     | `"l2"`                                             |
| `index_type`           | `string`                    |  yes     | `"exact"` \| `"hnsw"` \| `"hybrid"`                |
| `rerank_policy`        | `string`                    |  yes     | `"none"` \| `"exact"` (REQUIRED `"exact"` when `index_type ∈ {"hnsw","hybrid"}`) |
| `model_hash`           | `string`                    |  yes     | `"sha256:" + 64 hex`                               |
| `chunker_version`      | `string`                    |  yes     | non-empty                                          |
| `capabilities`         | object (see below)          |  yes     | `supports_exact = true`, `supports_reproducible_build = true`; `supports_ann = true` if HNSW section present; `supports_bm25 = true` if BM25 section present |
| `title`                | `string`                    |   no     | dataset display name                               |
| `version`              | `string`                    |   no     | dataset version (caller-defined)                   |
| `created`              | `string` (ISO-8601 UTC)     |   no     | `"1970-01-01T00:00:00Z"` in `--reproducible` mode  |
| `description`          | `string`                    |   no     |                                                    |
| `authors`              | `array<string>`             |   no     |                                                    |
| `license`              | `string`                    |   no     | SPDX identifier                                    |
| (`extra`)              | `map<string, json>`         |   no     | reader-tolerant additions; serialized BTreeMap     |

`capabilities` object:

```json
{
  "supports_exact":              true,
  "supports_ann":                false,
  "supports_bm25":               false,
  "supports_citations":          true,
  "supports_reproducible_build": true
}
```

`supports_exact` and `supports_reproducible_build` MUST be `true` in v1.
`supports_ann` and `supports_bm25` are not advisory: when `true`, the
file MUST carry the corresponding optional section (`0x07` / `0x08`)
and the manifest MUST declare a compatible `index_type`. The reader
enforces both directions.

### 9.1 Presets

The Python builder exposes four bundled presets (composable via
explicit kwargs):

| Preset       | text encoding | dtype     | HNSW | BM25 | Typical size vs `exact` | Recall@10 |
| ------------ | ------------- | --------- | ---- | ---- | ----------------------- | --------- |
| `exact`      | raw           | float32   | no   | no   | 1.000×                  | 1.000     |
| `compressed` | zstd          | float16   | no   | no   | ~0.35×                  | 1.000     |
| `tiny`       | zstd          | int8      | yes  | no   | ~0.29×                  | ~0.97     |
| `hybrid`     | zstd          | float32   | yes  | yes  | ~0.67×                  | ~0.99     |

Numbers above are measured on a 30,725-chunk PT-BR corpus (dim=384).
Per-knob overrides win over preset defaults; see
`python/tools/measure_presets.py` for the harness.

## 10. Reproducibility

When the writer is invoked with the reproducible flag (Rust:
`NestFileBuilder::reproducible(true)`; Python: `nest.build(...,
reproducible=True)`), the writer overrides `manifest.created` to the
constant string `"1970-01-01T00:00:00Z"` (`REPRODUCIBLE_CREATED`).

Two builds with identical inputs (chunks, embeddings, manifest fields
other than `created`, provenance JSON) MUST produce byte-identical
files. This is enforced by:

- `crates/nest-format/src/writer.rs::reproducible_mode_overrides_created`
- `tests/test_e2e.py::test_reproducible_builds_match_byte_for_byte`

The provenance JSON is NOT rewritten by reproducible mode — callers are
responsible for keeping provenance deterministic if they want
bit-for-bit equality. The legacy converter at `python/convert_legacy.py`
demonstrates this.

## 11. Version skew

A reader rejects a file whose `format_version` or `schema_version` is
strictly greater than the reader's own `NEST_FORMAT_VERSION` /
`NEST_SCHEMA_VERSION`. A reader accepts files with equal or smaller
versions, on the assumption that earlier versions used a strict subset
of the current contract.

```text
file.format_version > reader.NEST_FORMAT_VERSION  → UnsupportedFormatVersion
file.schema_version > reader.NEST_SCHEMA_VERSION  → UnsupportedSchemaVersion
header.version_major != 1                         → UnsupportedVersion
header.version_minor > reader.NEST_VERSION_MINOR  → UnsupportedVersion
```

## 12. `chunk_id` derivation

The `chunk_id` is a domain-separated SHA-256 over the canonical text,
its original span, and the chunker version. The preimage layout is
fixed by spec — anything else lets two runs of the same chunker produce
different IDs for the same content, which breaks reproducibility.

```text
preimage =
    "nest:chunk_id:v1\n"                  (17 bytes ASCII, includes the newline)
    u32 LE  len(canonical_text)
    bytes   canonical_text
    u32 LE  len(source_uri)
    bytes   source_uri
    u64 LE  byte_start
    u64 LE  byte_end
    u32 LE  len(chunker_version)
    bytes   chunker_version

chunk_id = "sha256:" + lower_hex(SHA-256(preimage))
```

## 13. `content_hash`

`content_hash` is a stable identifier for the *content* of a `.nest`
file, independent of the wire encoding chosen for the canonical
sections AND independent of file-level metadata that may change
without the data changing (e.g. the `created` timestamp). It is the
input to the citation URI (§14).

```text
ordered = sort canonical-section-name lexicographically:
          [chunk_ids, chunks_canonical, chunks_original_spans,
           embeddings, provenance, search_contract]

h = SHA-256()
for (id, name) in ordered:
    decoded = decode_payload(section.encoding, section.physical_bytes)
    h.update(u32 LE len(name))
    h.update(name as UTF-8)
    h.update(u64 LE len(decoded))
    h.update(decoded)

content_hash = "sha256:" + lower_hex(h.finalize())
```

Key properties:

- The order is fixed by spec (sorted alphabetically) so adding optional
  sections (HNSW, BM25) cannot reshuffle existing files' content_hash —
  optional sections are NOT in the canonical list.
- `decoded` is what a reader would consume after wire decoding: zstd
  decompressed, raw borrowed verbatim. So zstd-compressing
  `chunks_canonical` does NOT change `content_hash`.
- For dtype-quantized embeddings (float16, int8), the on-disk bytes ARE
  the canonical representation — `decoded` equals the physical bytes.
  Therefore an int8 corpus and its float32 source have **different**
  `content_hash` (the lossy quantization is part of the content).
- Padding is NOT included.

## 14. Citation URI

```text
nest://<content_hash>/<chunk_id>
```

Where `content_hash` and `chunk_id` are full `sha256:<hex>` strings
(NOT just the hex tails). Resolution (`nest cite`) requires:

- `content_hash` matches `view.content_hash_hex()` of the target file.
- `chunk_id` is present in the file's `chunk_ids` section.

A successful resolve returns `(canonical_text, source_uri, byte_start,
byte_end)`. Citations are stable across builds of the same content even
if the wrapper file is rebuilt with different `created`/metadata.

## 15. Errors

The reader emits a typed `NestError` per failure mode. The runtime
wraps these in `RuntimeError`. Categories:

| Category   | Variants                                                        |
| ---------- | --------------------------------------------------------------- |
| Format     | `MagicMismatch`, `UnsupportedVersion`, `UnsupportedFormatVersion`, `UnsupportedSchemaVersion`, `UnsupportedSectionVersion`, `UnsupportedSectionEncoding`, `MalformedSectionPayload`, `FileTruncated`, `FileSizeMismatch`, `UnexpectedEof`, `MissingRequiredSection`, `SectionOffsetOutOfBounds`, `SectionMisaligned`, `SectionNotFound`, `SectionCountMismatch` |
| Hash       | `InvalidHeaderChecksum`, `SectionChecksumMismatch`, `FooterHashMismatch` |
| Manifest   | `ManifestInvalid`, `UnsupportedDType`, `UnsupportedMetric`, `UnsupportedScoreType`, `UnsupportedNormalize`, `UnsupportedIndexType`, `UnsupportedRerankPolicy`, `InvalidModelHash` |
| Embedding  | `EmbeddingSizeMismatch`, `InvalidEmbeddingValue`               |
| Query      | `EmptyQuery`, `ZeroNormQuery`, `InvalidEmbeddingValue` (in query), `DimensionMismatch`, `QueryValidation` |
| Bookkeeping| `InvalidK`, `Io`, `Json`, `FileNotFound`, `InvalidInput`, `UnsupportedFeature` |

No panic path is acceptable. A reader that hits an unexpected condition
MUST return a `NestError`.

## 16. Search contract

v1 ships three search modes; the manifest's `index_type` declares
which is the default for `nest search-text`:

| Mode     | Selector                           | What it does                                                    | Recall guarantee                          |
| -------- | ---------------------------------- | --------------------------------------------------------------- | ----------------------------------------- |
| `exact`  | `index_type = "exact"`             | flat dot product against every vector                           | `recall == 1.0` always                    |
| `hnsw`   | `index_type = "hnsw"`              | HNSW retrieves `ef_search` candidates, exact dot-product rerank | candidate-pool dependent; runtime reports |
| `hybrid` | `index_type = "hybrid"`            | BM25 ∪ HNSW candidates, RRF fusion, exact dot-product rerank    | candidate-pool dependent; runtime reports |

Common runtime guarantees (all three modes):

- Query `[f32; embedding_dim]` is L2-normalized internally (the file's
  embeddings are L2-normalized at build time, so the resulting score is
  cosine ∈ `[-1, 1]`).
- The returned `score` is the **real cosine** value, not a transformed
  proxy. ANN/hybrid get there by reranking the candidate set with the
  exact dot product against the `embeddings` section.
- The dot product accumulates in `f32` regardless of on-disk dtype
  (float16 and int8 are widened lane-by-lane in SIMD).
- Top-`k` is a stable sort by score descending, breaking ties by index
  ascending.
- `truncated == true` iff `k < n_embeddings`.
- `k_returned == min(k, n_embeddings)`.
- Validations: `k > 0`, query non-empty, `query.len() == embedding_dim`,
  no `NaN`/`Inf` in query, query L2 norm > 0.

`SearchResult.recall` is `1.0` for `exact`. For `hnsw` / `hybrid` the
runtime returns `f32::NAN` to make explicit that recall depends on the
candidate-pool size and is the caller's job to measure (the CLI
`benchmark --ann <ef>` does exactly that against the same file's
exact path).

### 16.1 SIMD dispatch

The dot product is dispatched at first call to one of:

- `avx2` (x86_64 with AVX2 + FMA), 8-lane `f32x8` FMA accumulator
- `neon` (aarch64), 4-lane `f32x4` FMA accumulator with `vfmaq_f32`
- `scalar` (fallback), portable autovectorizable loop

Selection is cached after the first call. Set `NEST_FORCE_SCALAR=1`
to disable SIMD entirely (used for before/after benches without
recompilation). The SIMD path always produces the same ranking as
scalar within `1e-4` per dot product (tested across dim ∈ {4, 7, 16,
31, 64, 384}).

Each `SearchHit` carries:

```text
chunk_id        (sha256:<hex>)
score           (f32, real cosine)
score_type      ("cosine")
source_uri      (from chunks_original_spans)
offset_start    (byte_start)
offset_end      (byte_end)
embedding_model (from manifest)
index_type      ("exact")
reranked        (false in v1)
file_hash       (sha256:<hex>, the file's footer hash)
content_hash    (sha256:<hex>, see §13)
citation_id     ("nest://" + content_hash + "/" + chunk_id)
```

## 17. CLI surface

| Command                                       | Behavior                                                       |
| --------------------------------------------- | -------------------------------------------------------------- |
| `nest inspect <f>`                            | dump header, section table (with encoding name), manifest, hashes |
| `nest validate <f>`                           | full integrity check (header / sections / footer / manifest)   |
| `nest stats <f>`                              | size, chunk count, dim, model, hashes, per-section sizes, dtype, SIMD backend |
| `nest search <f> Q -k K`                      | exact top-k search; Q is a JSON array of f32                   |
| `nest search-text <f> "query" -k K`           | embed query text via `python/embed_query.py` (manifest's model), then run the declared `index_type` (exact / hnsw / hybrid) |
| `nest search-ann <f> Q -k K --ef N`           | force HNSW path (falls back to exact if no HNSW section)       |
| `nest benchmark <f> -q N -k K [--ann EF]`     | latency stats over N random queries; with `--ann` also runs ANN bench and computes recall@k vs exact |
| `nest cite <f> URI`                           | resolve `nest://...` into `(text, span, hashes)`               |

## 18. Python bindings

Built into `python/_nest.so` (PyO3, abi3-py312). High-level wrapper at
`python/nest.py` re-exports:

| Python symbol                                            | Backing                                                     |
| -------------------------------------------------------- | ----------------------------------------------------------- |
| `nest.open(path) -> NestFile`                            | `MmapNestFile::open`                                        |
| `NestFile.search(q, k)`                                  | exact flat search (recall=1.0)                              |
| `NestFile.search_ann(q, k, ef)`                          | HNSW + exact rerank; falls back to exact if no HNSW section |
| `NestFile.search_hybrid(q, query_text, k, candidates)`   | BM25 ∪ vector → RRF → exact rerank                          |
| `NestFile.inspect()`                                     | dict of header / sections / manifest / hashes / SIMD backend |
| `NestFile.validate()`                                    | re-runs reader validation; raises on any failure            |
| `NestFile.embedding_dim`                                 | header field                                                |
| `NestFile.n_embeddings`                                  | header field                                                |
| `NestFile.dtype`                                         | `"float32"` \| `"float16"` \| `"int8"`                      |
| `NestFile.simd_backend`                                  | `"scalar"` \| `"avx2"` \| `"neon"`                          |
| `NestFile.has_ann` / `NestFile.has_bm25`                 | bool                                                        |
| `NestFile.file_hash`                                     | footer hash (`sha256:<hex>`)                                |
| `NestFile.content_hash`                                  | content hash (§13)                                          |
| `nest.build(..., preset="exact"\|"compressed"\|"tiny"\|"hybrid")` | `NestFileBuilder` glue with preset shortcuts plus per-knob overrides (`text_encoding`, `dtype`, `with_hnsw`, `with_bm25`, `hnsw_m`, `hnsw_ef_construction`, `hnsw_seed`) |
| `nest.chunk_id(...)`                                     | `chunk_id` derivation (§12) for dedup                       |

## 19. Reference artefacts

- Golden minimal fixture: `crates/nest-format/tests/fixtures/golden_v1_minimal.nest`
  (1366 bytes; `file_hash`, `content_hash`, `chunk_id`, length asserted in
  `crates/nest-format/tests/golden.rs`). Frozen at the original v1 shape
  (raw + float32 + 6 required sections) — every reader change MUST keep
  it byte-identical to prove backward compatibility.
- Regenerator: `cargo run -p nest-format --example regen_golden` rewrites
  the fixture and prints the new constants.
- Reproducibility test: `tests/test_e2e.py::test_reproducible_builds_match_byte_for_byte`.
- Encoding tests: `crates/nest-format/src/writer.rs::tests` covers
  `zstd_encoding_preserves_content_hash`, `float16_embeddings_roundtrip`,
  `int8_embeddings_roundtrip`, `rejects_zstd_embeddings`.
- SIMD parity tests: `crates/nest-runtime/src/simd.rs::tests` covers
  `f32_simd_matches_scalar`, `f16_simd_matches_scalar`,
  `i8_simd_matches_scalar` across multiple dims.
- HNSW tests: `crates/nest-runtime/src/ann.rs::tests` covers
  `small_index_recall_against_exact`, `serialize_roundtrip`,
  `deterministic_build_same_seed`.
- BM25 tests: `crates/nest-runtime/src/bm25.rs::tests` covers
  `tokenizer_handles_pt_br`, `bm25_finds_relevant_doc`,
  `bm25_serialize_roundtrip`, `rrf_union_combines_sources`.
- Acceptance harness: `python/tools/measure_presets.py` rebuilds the
  baseline corpus at each preset and reports size, recall@k, score
  drift, p50/p95/p99 latency.
- Legacy SQLite dataset: `data/truw_ptbr.nest` (28 MB) →
  `data/truw_ptbr.v1.nest` (73.73 MB, 19,769 chunks, dim 384) via
  `python/convert_legacy.py`.
- Pipeline reference: `python/builder.py` (chunker + SQLite scratch +
  emit + auto-validate). `BuildConfig.preset` selects one of the four
  bundled presets; per-knob overrides win.

## 20. Reserved encodings / IDs

For future use within v1 (not yet implemented):

- Section IDs `0x09`-`0xFF` are reserved.
- Encoding values `4`-`255` are reserved. New encodings MUST keep the
  invariant: section `checksum` over physical bytes, `content_hash` over
  decoded bytes.
