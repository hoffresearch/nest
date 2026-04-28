# nest

portable, hash-verified, mmap-friendly container for semantic knowledge: canonical chunks, their embeddings, original-document spans, optional ANN and lexical indices, and a runtime contract that pins every search-time decision.

`.nest` is frozen at v1. the byte-by-byte specification is in `doc/spec.md`. the rust implementation in `crates/nest-format` is the canonical encoder/decoder.

## principles

reader-first. every read path validates magic, header checksum, section checksums, footer hash, manifest contract, required-section presence, and embedding values. failures return typed errors, never panics.

hash everything. header, every section's physical bytes, the file as a whole, and a `content_hash` over the canonical sections' decoded bytes. four hashes, four failure modes.

recall before throughput. exact flat search is the recall=1.0 ground truth and is always available. ANN (HNSW) and hybrid (BM25 union vector) are optional and always rerank candidates with the exact dot product, so the final score is the real cosine value.

reproducible. with `reproducible=True`, two builds with identical inputs produce byte-identical files. the manifest `created` timestamp is overridden to `1970-01-01T00:00:00Z`. the HNSW graph uses a fixed seed; the BM25 index is sorted by alphabetical term order. callers must keep `provenance` deterministic for bit-for-bit equality.

citable. every chunk has a stable `chunk_id` derived from a domain-separated SHA-256 preimage. every hit is resolvable via `nest://<content_hash>/<chunk_id>`. citations are stable across wire encodings because `content_hash` is over decoded bytes, not stored bytes.

offline-first. the runtime never opens a socket. queries are answered from mmap. the embedding model is identified by a granular fingerprint (config + tokenizer + weights + pooling + dim + normalize), and `--model-path` lets callers operate without ever touching huggingface.

## binary format

all multi-byte integers are little-endian unsigned. SHA-256 is the only hash. hex is lowercase. format: `sha256:<64 hex chars>`. sections are 64-byte aligned (padding is zero, not part of any section's checksum). strings are length-prefixed UTF-8: u32 LE length, then bytes, no NUL terminator.

file layout:

```
[0 .. 128)                  NestHeader (128 bytes)
[128 .. 128+count*32)       SectionTable (32 bytes per entry)
[manifest_offset ..)        Manifest (JCS canonical JSON, no whitespace)
[sections ...]              required + optional sections, each 64-byte aligned
[file_size-40 .. file_size) Footer (40 bytes)
```

header (`NestHeader`, 128 bytes):

```
Offset  Size  Field
0       4     magic                ASCII "NEST" (0x4E455354)
4       2     version_major        1
6       2     version_minor        0
8       4     flags                0 in v1
12      4     embedding_dim        > 0
16      8     n_chunks
24      8     n_embeddings         always == n_chunks in v1
32      8     file_size            total bytes
40      8     section_table_offset always 128
48      8     section_table_count
56      8     manifest_offset
64      8     manifest_size
72      8     header_checksum      SHA-256(header[0..72] ++ header[80..128]), first 8 bytes
80      48    reserved             all zero in v1
```

section table entry (`SectionEntry`, 32 bytes):

```
Offset  Size  Field
0       4     section_id          one of 0x01..0x06 (required), 0x07..0x08 (optional)
4       4     encoding            0=raw, 1=zstd, 2=float16, 3=int8
8       8     offset              must be 64-byte aligned
16      8     size                payload length, excludes padding
24      8     checksum            SHA-256(payload), first 8 bytes (over physical bytes)
```

required sections (sorted by `section_id` in the table, alphabetical by name for `content_hash`):

```
0x01  chunk_ids                length-prefixed UTF-8 strings (sha256:<64 hex>)
0x02  chunks_canonical         length-prefixed UTF-8 strings
0x03  chunks_original_spans    per-chunk (source_uri, byte_start, byte_end)
0x04  embeddings               raw bytes per dtype, shape [n_chunks, embedding_dim]
0x05  provenance               versioned JSON blob (free-form, deterministic)
0x06  search_contract          versioned JSON: metric, score_type, normalize, index_type, rerank_policy
```

optional sections (skipped by older readers, not part of `content_hash`):

```
0x07  hnsw_index               serialized HNSW graph for ANN search
0x08  bm25_index               inverted index for lexical search and hybrid fusion
```

every section except `embeddings` (0x04) starts with a 12-byte prefix: u32 version (1) + u64 count. `embeddings` has no prefix; for `dtype=float32` payload is exactly `n * dim * 4` bytes, for `float16` exactly `n * dim * 2`, for `int8` it's a small per-vector scale prefix followed by `n * dim` i8 bytes.

footer (`NestFooter`, 40 bytes):

```
Offset  Size  Field
0       8     footer_size  always 40
8       32    file_hash    SHA-256(file[0..file_size-40])
```

## section encodings

four encodings live within v1; the format version does not bump because old files (raw + float32 + 6 required sections) still load unchanged.

```
encoding=0  raw       text sections, default
encoding=1  zstd      text sections (chunks_canonical, chunks_original_spans, provenance, search_contract, bm25_index). reader decodes transparently.
encoding=2  float16   embeddings only. writer converts f32 to f16 at build time. runtime decodes lane-by-lane into f32, accumulates in f32.
encoding=3  int8      embeddings only. per-vector f32 scale plus n*dim i8 quantized values. lossy. always paired with rerank against the original f32 path or with HNSW + exact rerank.
```

zstd never applies to embeddings (the runtime mmaps and SIMD-reads them straight from disk). embeddings sections are always `encoding ∈ {0, 2, 3}`.

## the four hashes

each catches a different class of failure.

```
header_checksum     SHA-256 of header bytes with checksum slot zeroed; first 8 bytes
section.checksum    SHA-256 of physical bytes; first 8 bytes. catches disk corruption / bit-flip
file_hash           SHA-256 of file[0..file_size-40] (everything before the footer)
content_hash        SHA-256 over canonical sections in alphabetical-by-name order, each domain-separated by length-prefixed name + length-prefixed decoded payload
```

`content_hash` is over decoded bytes. if you re-encode the same logical content with zstd, `content_hash` stays the same. that's what makes citation URIs stable across encodings: `nest://content_hash/chunk_id` points at content, not at a copy.

## chunk_id derivation

```
preimage = "nest:chunk_id:v1\n"
           u32 LE len(canonical_text)  bytes canonical_text
           u32 LE len(source_uri)       bytes source_uri
           u64 LE byte_start
           u64 LE byte_end
           u32 LE len(chunker_version)  bytes chunker_version

chunk_id = sha256:<hex( SHA-256(preimage) )>
```

deterministic over the inputs; rebuilds of the same content yield the same id.

## citation URI

```
nest://<content_hash>/<chunk_id>
```

both halves are full `sha256:<hex>` strings. `nest cite` rejects citations whose `content_hash` does not match the file's `content_hash`.

## manifest

JCS canonical JSON. no whitespace. declaration-ordered known fields, BTreeMap order for `extra`.

required fields: `format_version` (1), `schema_version` (1), `embedding_model`, `embedding_dim`, `n_chunks`, `dtype` ("float32" | "float16" | "int8"), `metric` ("ip"), `score_type` ("cosine"), `normalize` ("l2"), `index_type` ("exact" | "hnsw" | "hybrid"), `rerank_policy` ("none" | "exact"), `model_hash` (sha256:<64 hex>), `chunker_version`, `capabilities` { `supports_exact: true`, `supports_ann`, `supports_bm25`, `supports_citations: true`, `supports_reproducible_build: true` }.

optional fields: `title`, `version`, `created`, `description`, `authors`, `license`. extra keys go in a BTreeMap serialized after known fields.

with `reproducible=True`, `created` is overridden to `1970-01-01T00:00:00Z`.

## search contract

three search paths, all returning `SearchHit` with the real cosine score.

exact (always available, recall=1.0):
- query is L2-normalized internally
- score is real cosine
- top-k stable sort by score descending; ties broken by index ascending
- `truncated == true` iff `k < n_embeddings`

ANN via HNSW (when section 0x07 is present):
- HNSW pulls `ef_search` candidates
- runtime reranks the candidates with the exact dot product against the embeddings section
- final score is the real cosine, not an ANN proxy
- recall is candidate-set dependent; runtime exposes `--ef` for tuning

hybrid via BM25 union HNSW (when both 0x07 and 0x08 are present):
- vector path pulls `candidates_per_path` candidates from HNSW (or exact if no HNSW)
- BM25 path pulls `candidates_per_path` candidates over tokenized chunk text
- union via reciprocal-rank fusion (RRF, k=60)
- exact rerank on the union, top-k by real cosine

query validation: `k > 0`, query non-empty, `len(query) == embedding_dim`, no NaN/Inf, `norm > 0`.

`SearchHit` carries: `chunk_id`, `score` (f32 cosine in `[-1, 1]`), `score_type` ("cosine"), `source_uri`, `offset_start`, `offset_end`, `embedding_model`, `index_type` ("exact" | "hnsw" | "hybrid"), `reranked` (true for hnsw and hybrid), `file_hash`, `content_hash`, `citation_id`.

## SIMD dispatch

three dot-product backends per dtype, dispatched at runtime:

```
                AVX2 (x86_64)        NEON (aarch64)        scalar
f32 . f32       8 lanes              4 lanes               autovec
f32 . f16       8 lanes (load+cvt)   4 lanes (load+cvt)    autovec
f32 . i8        16 lanes (i8->i32)   16 lanes (i8->i32)    autovec
```

detection happens once at runtime via `is_x86_feature_detected!` / `is_aarch64_feature_detected!`. accumulators are always f32 regardless of dtype. set `NEST_FORCE_SCALAR=1` to force the scalar fallback for A/B benchmarks.

## errors

`NestError` (format crate): `MagicMismatch`, `UnsupportedVersion`, `UnsupportedFormatVersion`, `UnsupportedSchemaVersion`, `UnsupportedSectionVersion`, `UnsupportedSectionEncoding`, `MalformedSectionPayload`, `FileTruncated`, `FileSizeMismatch`, `UnexpectedEof`, `MissingRequiredSection`, `SectionOffsetOutOfBounds`, `SectionMisaligned`, `SectionNotFound`, `SectionCountMismatch`, `InvalidHeaderChecksum`, `SectionChecksumMismatch`, `FooterHashMismatch`, `ManifestInvalid`, `UnsupportedDType`, `UnsupportedMetric`, `UnsupportedScoreType`, `UnsupportedNormalize`, `UnsupportedIndexType`, `UnsupportedRerankPolicy`, `InvalidModelHash`, `EmbeddingSizeMismatch`, `InvalidEmbeddingValue`, `DimensionMismatch`, `EmptyQuery`, `ZeroNormQuery`, `InvalidK`, `Json`, `Io`, `FileNotFound`, `InvalidInput`, `UnsupportedFeature`, `QueryValidation`.

`RuntimeError` (runtime crate): wraps `NestError` plus `DimensionMismatch`, `InvalidK`, `EmptyQuery`, `ZeroNormQuery`, `InvalidQueryValue`, `Io`.

no panic path is acceptable in library code.

## workspace

```
crates/nest-format     standalone library (layout, sections, manifest, reader, writer, encoding, chunk, hashing)
crates/nest-runtime    depends on nest-format (mmap-backed search via MmapNestFile, ann::HnswIndex, bm25::Bm25Index, simd dispatcher)
crates/nest-cli        depends on nest-format + nest-runtime (clap binary, 8 subcommands)
crates/nest-python     depends on nest-format + nest-runtime (cdylib, PyO3 abi3-py312)
python/nest.py         dynamic loader, re-exports NestFile, SearchHit, open, build, chunk_id
python/builder.py      Pipeline class: chunker + SQLite scratch cache + emit + auto-validate
python/model_fingerprint.py   reproducible model fingerprint (config + tokenizer + weights + pooling + dim + normalize)
python/embed_query.py  CLI helper called by `nest search-text`; emits structured JSON with vector + fingerprint
python/convert_legacy.py      CLI converter from legacy SQLite .nest to v1 binary
python/tools/         ingestion (`nest_build_corpus.py`), regression measurement (`measure_presets.py`, `compare_measure.py`)
scripts/release_check.sh      end-to-end CI gate
```

each crate sub-module is split into directories of files, each at most 300 lines (see `kdb/adr/0011`).

## rust API (nest-format)

```rust
pub use chunk::{ChunkInput, chunk_id};
pub use encoding::{Int8EmbeddingsView, decode_payload, f16_bytes_to_f32};
pub use error::{NestError, Result};
pub use layout::*;
pub use manifest::{Capabilities, Manifest};
pub use reader::NestView;
pub use sections::{OriginalSpan, SearchContract, decode_chunk_ids, decode_chunks_canonical, decode_chunks_original_spans, decode_provenance, decode_search_contract};
pub use writer::{EmbeddingDType, NestFileBuilder, SectionEncoding};
```

builder API:

```rust
NestFileBuilder::new(manifest: Manifest) -> Self
    .add_chunk(chunk: ChunkInput) -> Self
    .add_chunks(chunks: impl IntoIterator<Item = ChunkInput>) -> Self
    .with_provenance(value: serde_json::Value) -> Self
    .text_encoding(SectionEncoding) -> Self        // Raw or Zstd
    .embedding_dtype(EmbeddingDType) -> Self       // Float32, Float16, or Int8
    .hnsw_index(bytes: Vec<u8>) -> Self            // attach optional 0x07
    .bm25_index(bytes: Vec<u8>) -> Self            // attach optional 0x08
    .reproducible(on: bool) -> Self
    .build_bytes() -> Result<Vec<u8>>
    .write_to_path(path: impl AsRef<Path>) -> Result<()>
```

reader API:

```rust
NestView::from_bytes(data: &[u8]) -> Result<Self>     // zero-copy view; validates everything
    .get_section_data(section_id: u32) -> Result<&[u8]>     // physical bytes (may be encoded)
    .decoded_section(section_id: u32) -> Result<Cow<[u8]>>  // decoded bytes (zstd-aware)
    .validate_embeddings_values() -> Result<()>             // walk for NaN/Inf
    .search_contract() -> Result<SearchContract>            // cross-checked against manifest
    .file_hash_hex() -> String
    .content_hash_hex() -> Result<String>
```

## rust API (nest-runtime)

```rust
MmapNestFile::open(path: &Path) -> Result<Self, RuntimeError>
    .embedding_dim() -> usize
    .n_embeddings() -> usize
    .dtype() -> DType                              // Float32, Float16, Int8
    .file_hash() -> &str
    .content_hash() -> &str
    .simd_backend() -> SimdBackend                 // Avx2, Neon, Scalar
    .declared_index_type() -> &str                 // "exact" | "hnsw" | "hybrid"
    .has_ann() -> bool
    .has_bm25() -> bool
    .search(query: &[f32], k: i32) -> Result<SearchResult, RuntimeError>
    .search_ann(query: &[f32], k: i32, ef_search: usize) -> Result<SearchResult, RuntimeError>
    .search_hybrid(query_vec: &[f32], query_text: &str, k: i32, candidates_per_path: usize) -> Result<SearchResult, RuntimeError>
    .madvise_cold()                                // posix_madvise(MADV_DONTNEED) hint, unix only
    .inspect_json() -> Result<String, RuntimeError>
    .revalidate() -> Result<(), RuntimeError>
```

`HnswIndex` and `Bm25Index` are public types in `nest_runtime::ann` and `nest_runtime::bm25` for callers that want to build indices ahead of writing.

## python API

```python
import sys; sys.path.insert(0, "python"); import nest

db = nest.open(path)                                    # NestFile

hits = db.search(qvec, k=5)                             # exact
hits = db.search_ann(qvec, k=5, ef=100)                 # HNSW + exact rerank
hits = db.search_hybrid(qvec, query_text, k=5,
                        candidates=200)                  # BM25 union vector + exact rerank

db.embedding_dim          # int
db.n_embeddings           # int
db.dtype                  # "float32" | "float16" | "int8"
db.simd_backend           # "avx2" | "neon" | "scalar"
db.has_ann                # bool
db.has_bm25               # bool
db.file_hash              # sha256:<hex>
db.content_hash           # sha256:<hex>

db.validate()             # raises ValueError on any failure
db.inspect()              # full JSON dump as a Python dict
```

`SearchHit` attributes: `chunk_id`, `score`, `score_type`, `source_uri`, `offset_start`, `offset_end`, `embedding_model`, `index_type`, `reranked`, `file_hash`, `content_hash`, `citation_id`.

build:

```python
nest.build(
    output_path,
    embedding_model,
    embedding_dim,
    chunker_version,
    model_hash,
    chunks,                       # [{canonical_text, source_uri, byte_start, byte_end, embedding}]
    *,
    title=None, version=None, created=None, description=None,
    authors=None, license=None, provenance=None, reproducible=False,
    preset="exact",               # "compressed" | "tiny" | "hybrid"
    text_encoding=None,           # "raw" | "zstd" overrides preset
    dtype=None,                   # "float32" | "float16" | "int8" overrides preset
    with_hnsw=None, with_bm25=None,
    hnsw_m=16, hnsw_ef_construction=400, hnsw_seed=42,
)

nest.chunk_id(canonical_text, source_uri, byte_start, byte_end, chunker_version) -> str
```

builder.Pipeline:

```python
builder.Pipeline(cfg: BuildConfig, embedder: Callable, scratch_db: str | None)
    .add(spec: ChunkSpec) -> None
    .add_many(specs: Iterable[ChunkSpec]) -> None
    .emit(*, provenance: dict | None = None) -> str
    .close() -> None

builder.chunk_text(text, source_uri, *, max_chars=512, overlap=0) -> list[ChunkSpec]
builder.BuildConfig(output_path, embedding_model, embedding_dim, chunker_version, model_hash,
                    preset="exact", reproducible=True, ...)
builder.EmbeddingCache(path).get(chunk_id, dim) | .put(chunk_id, embedding) | .close()
```

`model_fingerprint`:

```python
from model_fingerprint import compute_model_fingerprint, fingerprint_to_model_hash, resolve_model_dir

md = resolve_model_dir("sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2")
fp = compute_model_fingerprint(md, model_id="...")
model_hash = fingerprint_to_model_hash(fp)
```

## CLI

```
nest inspect     <file> [--json]                    header, manifest, hashes
nest validate    <file>                             full integrity check
nest stats       <file>                             sizes, counts, dtype, model, simd backend
nest search      <file> <qvec> -k K                 exact top-k, query is JSON array of f32
nest search-ann  <file> <qvec> -k K --ef N          HNSW + exact rerank
nest search-text <file> "query" -k K                embed via python/embed_query.py, model_hash gate
                 [--model-path PATH] [--skip-model-hash-check]
nest benchmark   <file> -q N -k K [--ann EF] [--madvise-cold]
nest cite        <file> nest://<content_hash>/<chunk_id>
```

`search` takes a vector. `search-text` shells out to the python embedder, validates `embedding_model` + `embedding_dim` + `model_hash` against the manifest, then routes to the declared `index_type` (exact, hnsw, hybrid). a model mismatch fails with a typed error, never silently.

## install and build

requirements: rust edition 2024 (`rustc >= 1.85`), python 3.12+.

```sh
cargo build --release --workspace
cp target/release/lib_nest.dylib python/_nest.so   # macOS
cp target/release/lib_nest.so   python/_nest.so    # linux
```

no maturin. no PyPI package. the extension is a plain cdylib.

## test

```sh
cargo test --release --workspace                   # all rust tests, 134/134 in v0.2
cargo test -p nest-format                           # format crate
cargo test -p nest-runtime                          # runtime crate
cargo test -p nest-cli                              # CLI integration

python tests/test_e2e.py                            # python e2e (requires built .so)
python tests/test_builder.py                        # builder pipeline + cache
python tests/test_search_text_model_hash.py         # search-text model_hash gate (5 cases)

./scripts/release_check.sh                          # full pipeline + regression gates
```

no pytest. tests are plain scripts with `if __name__ == "__main__"`.

golden fixture: `crates/nest-format/tests/fixtures/golden_v1_minimal.nest` (1366 bytes, byte-frozen). regenerate with `cargo run -p nest-format --example regen_golden`.

## lint and format

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
ruff check .
ruff format --check .
```

## version skew policy

```
file.format_version > reader.NEST_FORMAT_VERSION   -> UnsupportedFormatVersion
file.schema_version > reader.NEST_SCHEMA_VERSION   -> UnsupportedSchemaVersion
header.version_major != 1                          -> UnsupportedVersion
header.version_minor > reader.NEST_VERSION_MINOR   -> UnsupportedVersion
encoding ∈ {1,2,3} on a v1 reader that predates v0.2 -> UnsupportedSectionEncoding
unknown section_id (e.g. 0x07/0x08) on a v0.1 reader -> skipped, file still loads
```

lower or equal versions are accepted.

## license

MIT. copyright Hoff Research.
