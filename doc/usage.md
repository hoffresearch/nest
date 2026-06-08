# usage

`nest` is a single-file binary container for distributing semantic knowledge bases. one file: chunks, canonical text, byte-spans, embeddings, search contract, hashes. copy it, share it, search it.

this guide covers the eight commands you'll actually use.

## 1. build a `.nest` from chunks

the python pipeline owns chunking, embedding, caching, and the final emit. the rust writer owns reproducibility, hashing, and deterministic byte layout.

```python
import sys; sys.path.insert(0, "python")
from builder import BuildConfig, ChunkSpec, Pipeline, chunk_text

def embed(specs):
    # plug in your sentence-transformers or candle / onnxruntime here
    from sentence_transformers import SentenceTransformer
    m = SentenceTransformer(cfg.embedding_model)
    return m.encode([s.canonical_text for s in specs],
                    normalize_embeddings=True).tolist()

cfg = BuildConfig(
    output_path="my_corpus.nest",
    embedding_model="sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
    embedding_dim=384,
    chunker_version="my-chunker/v1",
    model_hash="sha256:" + "0" * 64,    # see §7 for the real fingerprint
    preset="exact",                      # see §6 for preset choices
    reproducible=True,
)
pipe = Pipeline(cfg, embedder=embed, scratch_db="cache.sqlite")
for source_uri, text in documents:
    for spec in chunk_text(text, source_uri):
        pipe.add(spec)
pipe.emit()
```

for real-world examples: `python/convert_legacy.py` (SQLite to `.nest`) and `python/tools/nest_build_corpus.py` (7 PT-BR datasets to a unified `.nest`).

direct API (no chunker): `nest.build(output_path, embedding_model, embedding_dim, chunker_version, model_hash, chunks, preset="exact", reproducible=True)`.

## 2. validate

full integrity check: magic, header checksum, every section's SHA-256 (over physical bytes), footer hash (over the whole file), manifest schema, contract cross-check against the manifest, NaN/Inf walk over the embeddings.

```sh
nest validate my_corpus.nest
```

failure modes are typed (`SectionChecksumMismatch(0x04)`, `UnsupportedDType("bfloat16")`, etc.), never "best effort".

## 3. stats

sizes, dim, dtype, model, hashes, per-section bytes, the SIMD backend the runtime selected.

```sh
nest stats my_corpus.nest
```

## 4. inspect

header bytes, full section table, manifest as JSON. use `--json` for programmatic consumers (CI dashboards, drift detection):

```sh
nest inspect my_corpus.nest             # human-readable
nest inspect my_corpus.nest --json | jq # structured
```

schema: `{magic, version_major, version_minor, format_version, schema_version, embedding_dim, n_chunks, n_embeddings, file_size, manifest, sections[], file_hash, content_hash, simd_backend}`.

## 5. search

### exact path (vector input)

pass a query vector directly as a JSON array. recall = 1.0 by construction.

```sh
nest search my_corpus.nest "[0.1, 0.2, ...]" -k 10
```

### search by text

embed the query with the same model the corpus was built with (the manifest declares it), then route to the declared `index_type` (exact, hnsw, hybrid). the runtime cross-checks the embedder's `model_hash` against the manifest before running search and refuses on mismatch. see §7.

```sh
nest search-text my_corpus.nest "vacina contra covid funciona" -k 5
```

for tuning the candidate set: `--candidates N` (default `4*k`, min 64).

### force the ANN path

useful for debugging or measuring `ef_search` curves. falls back to exact if the file has no HNSW section.

```sh
nest search-ann my_corpus.nest "[0.1, 0.2, ...]" -k 10 --ef 200
```

## 6. presets

`preset=` selects a (text encoding, embedding dtype, optional ANN, optional BM25) bundle. per-knob overrides win, see `BuildConfig.text_encoding`, `.dtype`, `.with_hnsw`, `.with_bm25`, `.mrl_dim`.

| preset       | text encoding | embeddings  | ANN | BM25 | size_ratio | recall@10 |
|--------------|---------------|-------------|-----|------|-----------:|----------:|
| `exact`      | raw           | float32     | no  | no   |      1.000 |    1.0000 |
| `compressed` | zstd          | float16     | no  | no   |      0.339 |    1.0000 |
| `tiny`       | zstd          | int8        | yes | no   |      0.256 |    0.9920 |
| `micro`      | zstd          | mrl256-int8 | yes | no   |      0.223 |    0.8100 |
| `nano`       | zstd          | int4        | yes | no   |      0.209 |    0.9130 |
| `hybrid`     | zstd          | float32     | yes | yes  |      0.609 |    1.0000 |

numbers measured on the project's PT-BR fake-news corpus (n=30,725, dim=384), 100 queries, k=10 vs the float32 exact baseline (the published ladder `dat/measure/ladder.json`, gated against `dat/measure/baseline.json`). these are the honest current sizes after the text-codec repack (intpack chunk_ids/spans, bitpacked hnsw/bm25 payloads) shrank the indexed presets below the v0.2 figures: `tiny` 0.283 -> 0.256, `compressed` 0.350 -> 0.339, `hybrid` 0.668 -> 0.609. latency ranges (NEON, hot cache): exact p50 ~3.1 ms, tiny p50 ~1.2 ms, micro p50 ~0.8 ms, nano p50 ~2.1 ms, hybrid p50 ~4.0 ms.

the `exact`/`compressed`/`tiny`/`nano`/`hybrid` rows are direct `preset=` values; `micro` is the published name for the matryoshka size lever (the documented honest point `mrl256-int8`), built with `nest.build(text_encoding="zstd", dtype="int8", mrl_dim=256, with_hnsw=True)` and emitted by `measure_presets.py --variants ...,micro,...`.

pick `nano` for the smallest distributable file with recall above the nano floor: int4 block-64 embeddings (per-64-dim-group f16 absmax scales + packed 4-bit codes) take the embeddings section from int8's 11.92 MB down to 6.27 MB (~1.9x over int8, ~7.5x over float32). `nano`/`micro` require the effective `embedding_dim` divisible by 64. every sub-int8 preset (`micro`/`nano` and the whole mrl curve) is STORED-PRECISION: the 0x09 `embeddings_fp` rerank source is not wired, so the net-of-fp ratio equals the stored ratio and `score`/`recall@10` are real cosine AT THE STORED PRECISION (int4/int8), disclosed via `dtype` (and `mrl_dim`/`full_dim` for `micro`) in `nest stats` and on every result, never a bare-slab ratio. `micro` trades recall for size on this non-mrl MiniLM baseline (0.810 recall@10 at 0.223 ratio, see the curve below); pick it only when raw size beats the last ~10 recall points or once a real mrl-trained model lands. pick `tiny` when you want a smaller file than `compressed` with recall still above 0.99, `compressed` when you need lossless cosine + 3x compression, `hybrid` when queries include rare terms, proper nouns, or siglas that pure embeddings underweight, and `exact` when storage isn't the bottleneck and you want the recall=1.0 ground truth.

### matryoshka prefix truncation (`mrl_dim`)

`nest.build(..., mrl_dim=K)` (or `BuildConfig.mrl_dim`) slices each l2-normalized vector to its first `K` components and re-l2-normalizes the prefix BEFORE quantization (Qwen3/ST/BGE truncate-then-renormalize). this is the dimension axis: orthogonal to and multiplicative with the dtype levers. the stored `embedding_dim` becomes `K`, the source dim is recorded as `full_dim`, and both appear in `nest stats`. queries are striped at `K` too, so a full-dim query against a truncated file is a dimension mismatch; slice + renorm the query to `K` first. truncation is a pure deterministic op, so builds stay byte-identical; `content_hash` is over the truncated embeddings, so a citation is tied to its `mrl_dim` (never claimed stable across dims). int4 still needs the effective dim divisible by 64, so `mrl_dim` in {256, 192, 128} works with int4 but 96 does not (use int8/f16/f32 at 96).

matryoshka pays off on a model trained for it (information front-loads into the prefix). the shipped MiniLM corpus is NOT mrl-trained, so truncation costs real recall@10 there; the published ladder in `dat/measure/ladder.json` (100 queries, k=10) reports the honest curve and `python/tools/measure_presets.py` emits it (the default `--variants` are `compressed,tiny,micro,nano,hybrid` plus `mrl256/192/128-int8`, `mrl96-int8`, `mrl256/192/128-int4`):

| ladder        | size ratio | recall@10 |
|---------------|-----------:|----------:|
| `mrl256-int8` (`micro`) |     0.223  |   0.810   |
| `mrl192-int8` |     0.207  |   0.733   |
| `mrl128-int8` |     0.190  |   0.659   |
| `mrl96-int8`  |     0.182  |   0.574   |
| `mrl256-int4` |     0.191  |   0.777   |
| `mrl192-int4` |     0.183  |   0.713   |
| `mrl128-int4` |     0.174  |   0.627   |

on that baseline `nano` (full-dim int4) still beats every truncated point on recall, so reach for `mrl_dim` (the `micro` rung is `mrl256-int8`) when raw size matters more than the last ~10 recall points, or once an mrl-trained embedder is in play. `compare_measure.py` gates `micro`/`nano`/`mrl256-int8` conditionally (size_ratio <= 0.25; recall >= 0.78 for micro/mrl256-int8, >= 0.85 for nano), only when the run includes them.

## 7. model_hash and offline operation (`--model-path`)

`search-text` cross-checks three things before running search:

1. `manifest.embedding_model` (name) matches the embedder's report.
2. `manifest.embedding_dim` matches `len(vector)`.
3. `manifest.model_hash` matches the embedder's reproducible fingerprint.

layer 3 is the only one that catches the silent failure mode "same name + same dim + different snapshot, cosine-valid garbage". the fingerprint hashes a fixed list of inference-relevant files (`config.json`, `tokenizer.json`, `model.safetensors`, `1_Pooling/config.json`, etc.). see `python/model_fingerprint.py`.

build with a real fingerprint:

```python
from model_fingerprint import (
    compute_model_fingerprint, fingerprint_to_model_hash, resolve_model_dir,
)
md = resolve_model_dir("sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2")
fp = compute_model_fingerprint(md, model_id="sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2")
cfg.model_hash = fingerprint_to_model_hash(fp)
```

### fully offline search

distribute the model directory alongside the `.nest` (e.g. on a USB stick or in a sealed Docker image), then point `--model-path` at it on every search:

```sh
nest search-text my_corpus.nest "vacina contra covid" -k 5 \
    --model-path /mnt/models/paraphrase-multilingual-MiniLM-L12-v2
```

no HuggingFace cache hits, no network. the fingerprint is recomputed locally and verified against the manifest.

### pre-phase-3 corpora

files built with `model_hash = sha256:0...0` (the legacy placeholder) fail the strict gate by design. two options:

- rebuild with a real fingerprint (recommended).
- pass `--skip-model-hash-check` to proceed at your own risk. the search is still cosine-valid if you genuinely use the same embedding model, but there is no guarantee.

## 8. benchmark

random-query latency stats (mean, p50, p95, p99). with `--ann`, also runs ANN against the same queries and computes `recall@k (ANN vs exact)`. with `--madvise-cold`, runs an extra pass calling `posix_madvise(MADV_DONTNEED)` between queries: upper bound on cold-cache latency, not absolute cold (see `MmapNestFile::madvise_cold` docs).

```sh
nest benchmark my_corpus.nest -q 100 -k 10 --ann 100 --madvise-cold
```

typical output (n=30,725, dim=384, neon, int8):

```
Exact (100 queries, dim=384, dtype=int8, simd=neon) [hot]:
  p50: 1.28 ms  p95: 1.68 ms
Exact ... [madvise-cold]:
  p50: 1.95 ms  p95: 2.40 ms
ANN ef=100 (100 queries) [hot]:
  p50: 0.44 ms  p95: 0.62 ms
  recall@10 (ANN vs exact): 0.9920
```

## 9. citations

every search hit carries a stable `citation_id` of the form `nest://<content_hash>/<chunk_id>`. resolve it back to the canonical text and original byte span:

```sh
nest cite my_corpus.nest 'nest://sha256:1aa9.../sha256:8f314...'
```

`content_hash` is hashed over the **decoded** bytes, so a corpus stored with `text_encoding=zstd` produces the same `content_hash` as the same logical content stored raw. citations are stable across wire encodings.

## 10. release verification

```sh
./scripts/release_check.sh
```

runs the full pipeline: cargo test, clippy, fmt, all 3 python test suites, ruff, `measure_presets.py`, `compare_measure.py` against the committed baseline. exits non-zero on any failure.
