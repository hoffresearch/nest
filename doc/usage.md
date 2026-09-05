# usage

`nest` is a single-file binary container for distributing semantic knowledge bases. one file: chunks, canonical text, byte-spans, embeddings, search contract, hashes. copy it, share it, search it.

this guide covers the commands you'll actually use: the agent verbs `ask`, `retrieve` and `build` (the front door; they shell out to the offline python embedder or the forge), and the engine subcommands beneath them (validate, stats, inspect, media, search/search-ann/search-graph/search-space/search-text, benchmark, cite, doctor), which take a file and a vector and never run python. `nest --help` lists them in the same two groups.

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

### image and pdf corpora

image corpora live in the forge tooling layer because a vision tower needs torch, which the sovereign runtime does not take. the `.nest` they emit is an ordinary `.nest`, served by the same rust runtime from mmap.

the media travels inside the file. the encoded stream is stored as content-addressed blobs (section 0x14), each chunk carries the exact byte span it was embedded from (overlay 0x16), and the image vectors sit in their own named space (registry 0x15, slab in the 0x20-0x2F band) behind the `supports_multimodal` capability, gated by their own `model_hash` in isolation. these sections are excluded from `content_hash`, so adding media never moves an existing citation.

`python/tools/nest_build_image_corpus.py` letterboxes every image onto one canvas, encodes the sequence, embeds the DECODED frames, and writes one chunk per image or pdf page. embedding the decoded frames rather than the source pixels is deliberate: the index has to describe what a reader can actually get back.

```sh
.venv/bin/python python/tools/nest_build_image_corpus.py \
    --input-dir /path/to/dermoscopy_images \
    --dataset my-derm \
    --output corpora/my-derm.nest \
    --labels labels.csv
```

a corpus is one file. `corpora/my-derm.nest` carries the index, the media blobs, the span overlay, and the space registry; copying it moves the corpus intact. provenance (ordinals, origins, labels, media digests) rides inside as well.

`--width` is a ceiling, not a target: the canvas is clamped to the dataset's median source width, so a corpus is never upscaled. lower it to trade quality for size; raising it above the source does nothing but make the encoder pay for interpolated pixels.

`--gop-policy auto` (the default) probes a spaced sample of frames and lets the bytes decide between all-intra and inter coding. on every corpus measured so far (ph2, ham10000, wsi tiles, scanned pdf) the probe chose intra: unrelated images give inter-frame prediction nothing to find. inter stays available for genuinely sequential media. `--all-intra` forces every frame a keyframe and overrides the probe.

`--shard-size N` splits the stream into consecutive segments of about N frames, one blob per shard, which caps decode memory and improves cold seek on large corpora. `--order-similarity` tries a greedy nearest-neighbour frame order before encoding; measured on 1210 wsi tiles it cost 0.15 percent instead of helping, so it stays off by default.

`--backend av1` (the default) won the size-matched matrix; `--backend avif` writes one avif per image and is the only backend that accepts `--pix-fmt yuv444p`. `--crf` (default 35) sets the av1 rate, `--avif-quality` (default 35) the avif one. `--control` builds the letterbox-lossless png control corpus that codec cost is measured against.

`--dtype float32|float16|int8|int4` overrides the preset's vector dtype for the image space (int4 needs the dim divisible by 64). measured: quantization was not the driver of quality loss (the melanoma delta is identical at f16 and int8, and similar at int4), while the vectors themselves shrink 214 KB to 112.6 KB to 63.8 KB on ph2.

add `--pdf` to render pdf pages as the images; page numbers are kept in the manifest and in the citable text. for a non-dermatology domain pass `--model ViT-B-32 --pretrained openai`. the pretrained tag is required for bare architecture names, because open_clip answers a missing tag with random weights.

search with a query image or a clinical description, and optionally decode the matched frames back out:

```sh
.venv/bin/python python/tools/nest_search_image.py \
    --index corpora/my-derm.nest --query-image lesion.jpg -k 10 \
    --letterbox-query --save-frames hits/
```

`--query-text "..."` searches with a clinical description instead of an image, and `--letterbox-query` normalizes the query onto the corpus canvas before embedding. queries route through `search_space`, so the image space's own `model_hash` is checked against the manifest, in isolation from the default text space, before anything is scored. `--skip-model-check` bypasses that gate explicitly.

### measuring an image corpus

`python/tools/nest_image_eval.py` reports two rulers and keeps them apart, because they answer different questions:

- `identity` asks whether a source image retrieves its own frame. it measures rank stability under the codec and is inflated by construction, since the corpus contains the answer. on an uncompressed index it returns 1.000 by definition.
- `label` removes the query's own frame and scores how many of the remaining neighbours share its label. nothing in the corpus is the answer, so this is the one that reports retrieval quality. it is printed next to the random-pick baseline for the same label distribution, without which the number cannot be read.

neither means much alone. pass `--baseline` with the uncompressed control index (`--control` at build time) to get the delta, which is what the codec actually cost:

```sh
.venv/bin/python python/tools/nest_image_eval.py \
    --index corpora/my-derm.nest \
    --baseline corpora/my-derm-control.nest \
    -k 1 5 10 --out eval.json
```

measured in phase 6 (full matrix and intervals in `doc/changelog.md`): on ph2 (n=200) av1-intra crf35 compresses the media 86x for a mean label `precision@10` delta of -3.4 to -4.7 points whose interval crosses zero, but the melanoma class alone drops 16.9 points with a significant interval ([-25, -10]); on ham10000 (2000-sample) the media shrinks 151x for a mean delta of -1.5 [-3.5, +0.6], again with a significant melanoma cost (-10.7). the text-to-image ruler is harsher and honest: 44/60 correct top-10 clinical queries on the control falls to 22/60 at crf35, and the loss does not recover with rate. per-class floors matter more than the mean: report the interval and the worst class, not just the point.

`python/tools/nest_image_sweep.py` runs the variant matrix for you (av1-intra crf ladder, avif444, control, `dtype:` rungs, `av1-order`), records `nest_bytes` and the control's `media_bytes` per variant, and writes one consolidated comparison json:

```sh
.venv/bin/python python/tools/nest_image_sweep.py \
    --input-dir /path/to/images --dataset my-derm \
    --variants av1-intra-crf35,av1-intra-crf40,control,dtype:int8 \
    --labels labels.csv --out-dir sweep/ --out sweep/summary.json
```

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

### graph search (chunk-to-chunk)

seeds from the exact-cosine top-`ef`, expands a bounded breadth-first walk over the chunk-to-chunk graph (`--hops`), then exact-reranks the union. the graph only generates candidates; the returned score is real cosine (recall is not computed, the rerank guarantees the score). falls back to exact if the file has no `graph_adjacency` (0x0C) section. build a graph-carrying file with `nest.build(..., with_graph=True)` (default off); the section is additive and excluded from content_hash, so adding a graph never changes a citation.

```sh
nest search-graph my_corpus.nest "[0.1, 0.2, ...]" -k 10 --hops 2 --ef 100
```

### search a named space (multimodal)

`search-space` runs the per-space exact search over one named vector band (0x15 + 0x20+): image spaces, extra text spaces, mrl-sliced spaces. the query vector must be embedded with the space's model at the space's dim; an unknown space, a wrong dim, or (with `--expect-model-hash`) a wrong model are typed errors; never a silent fallback to the text path. the space names come from `nest stats` (the `spaces:` block) or `inspect --json` (the `spaces[]` array).

```sh
nest search-space my_corpus.nest "[0.1, ...]" --space "wemm-2b@256" -k 5
nest benchmark my_corpus.nest -q 100 -k 10 --space "wemm-2b@256"
```

### the flagship: ask and retrieve

`ask` and `retrieve` are the agent-native front door: text query in, cited answer out, no flags needed. they embed the query OFFLINE and route the embedder BY THE MANIFEST MODEL: a potion corpus keeps the potion static table (`python/forge/embed_query_potion.py`, the unchanged fast path), and a corpus whose default text space is any registry model (wemm, jina, clip; see §12) goes through `python/forge/embed_query_model.py`, which encodes the query with that model's query route and, for an mrl-truncated default space, slices + renormalizes to the manifest dim (`--mrl-dim`, passed automatically). both paths validate the embedder's `model_hash` against the manifest exactly like `search-text`, and route by manifest capability (exact if only embeddings, hnsw/hybrid/graph as the file advertises). every printed score IS the exact-cosine rerank value.

`ask` prints one low-cognitive-load cited answer:

```sh
nest ask my_corpus.nest "can I use this offline" -k 3
```

`--disclose answer` (default) prints the cited canonical text and a `nest://` citation, nothing else. `--disclose explain` ALSO prints the rerank-source honesty line: `real cosine` when the score is full precision, `real cosine at stored precision` for a lossy stored slab (float16/int8/int4) with no full-precision source, plus the route and per-path candidate counts.

`retrieve` is the agent-shaped surface: a json/jsonl answer-pack of cited spans.

```sh
nest retrieve my_corpus.nest "can I use this offline" -k 5 --format jsonl
```

each hit is `{chunk_id, score, score_type=cosine, source_uri, offset_start, offset_end, citation_id, text, file_hash, content_hash, rerank_source}`. the `score` is the exact rerank value (never a candidate-generator proxy), `text` is the tier-1 stored canonical text, and `citation_id` round-trips through `nest cite`. `--format json` emits a single pretty array instead of one object per line.

the embedder picks its interpreter in a fixed order: `NEST_PYTHON` if set, else the repo's `.venv/bin/python` (which carries the forge deps: numpy + tokenizers + the vendored potion table) discovered by walking up from the cwd, else `python3` on PATH. so the repo `.venv` is used automatically; set `NEST_PYTHON` only to force a specific interpreter. the selected interpreter is printed to stderr; and since discovery executes the nearest ancestor `.venv/bin/python`, set `NEST_PYTHON` explicitly if you run `nest` from inside an untrusted directory tree. point `--model-path` at a copied potion table dir for a fully sealed offline run.

the python convenience is `python python/forge/retrieve.py`: it builds a `.nest` from the cc0 demo corpus with the potion embedder, asks a question, and prints the cited answer with a `nest://` citation, all offline and deterministic (the one-gif demo).

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

numbers measured on the project's PT-BR fake-news corpus (n=30,725, dim=384), 100 queries, k=10 vs the float32 exact baseline (the published ladder `dat/measure/ladder.json`, gated against `dat/measure/baseline.json`). RULER CAVEAT: these `recall@10` figures use a SELF-PERTURBATION ruler (each query is a corpus vector plus tiny noise), so they measure rank-stability under quantization, NOT real-query retrieval, and are likely inflated; see the `ruler` field in `ladder.json`/`baseline.json` and the pending real-query (mteb-style) ruler (gate-zero). these are the honest current sizes after the text-codec repack (intpack chunk_ids/spans, bitpacked hnsw/bm25 payloads) shrank the indexed presets below the v0.2 figures: `tiny` 0.283 -> 0.256, `compressed` 0.350 -> 0.339, `hybrid` 0.668 -> 0.609. latency ranges (NEON, hot cache): exact p50 ~3.1 ms, tiny p50 ~1.2 ms, micro p50 ~0.8 ms, nano p50 ~2.1 ms, hybrid p50 ~4.0 ms.

the `exact`/`compressed`/`tiny`/`nano`/`hybrid` rows are direct `preset=` values; `micro` is the published name for the matryoshka size lever (the documented honest point `mrl256-int8`), built with `nest.build(text_encoding="zstd", dtype="int8", mrl_dim=256, with_hnsw=True)` and emitted by `measure_presets.py --variants ...,micro,...`.

pick `nano` for the smallest distributable file with recall above the nano floor: int4 block-64 embeddings (per-64-dim-group f16 absmax scales + packed 4-bit codes) take the embeddings section from int8's 11.92 MB down to 6.27 MB (~1.9x over int8, ~7.5x over float32). `nano`/`micro` require the effective `embedding_dim` divisible by 64. every sub-int8 preset (`micro`/`nano` and the whole mrl curve) is STORED-PRECISION: the 0x09 `embeddings_fp` rerank source is not wired, so the net-of-fp ratio equals the stored ratio and `score`/`recall@10` are real cosine AT THE STORED PRECISION (int4/int8), disclosed via `dtype` (and `mrl_dim`/`full_dim` for `micro`) in `nest stats` and on every result, never a bare-slab ratio. `micro` trades recall for size on this non-mrl MiniLM baseline (0.810 recall@10 at 0.223 ratio, see the curve below); pick it only when raw size beats the last ~10 recall points or once a real mrl-trained model lands. pick `tiny` when you want a smaller file than `compressed` with recall still above 0.99, `compressed` when you need lossless cosine + 3x compression, `hybrid` when queries include rare terms, proper nouns, or siglas that pure embeddings underweight, and `exact` when storage isn't the bottleneck and you want the recall=1.0 ground truth.

### matryoshka prefix truncation (`mrl_dim`)

`nest.build(..., mrl_dim=K)` (or `BuildConfig.mrl_dim`) slices each l2-normalized vector to its first `K` components and re-l2-normalizes the prefix BEFORE quantization (Qwen3/ST/BGE truncate-then-renormalize). this is the dimension axis: orthogonal to and multiplicative with the dtype levers. the stored `embedding_dim` becomes `K`, the source dim is recorded as `full_dim`, and both appear in `nest stats`. queries are striped at `K` too, so a full-dim query against a truncated file is a dimension mismatch; slice + renorm the query to `K` first. truncation is a pure deterministic op, so builds stay byte-identical; `content_hash` is over the truncated embeddings, so a citation is tied to its `mrl_dim` (never claimed stable across dims). int4 still needs the effective dim divisible by 64, so `mrl_dim` in {256, 192, 128} works with int4 but 96 does not (use int8/f16/f32 at 96).

matryoshka pays off on a model trained for it (information front-loads into the prefix). the shipped MiniLM corpus is NOT mrl-trained, so truncation costs real recall@10 there; the published ladder in `dat/measure/ladder.json` (100 queries, k=10) reports the honest curve (same self-perturbation ruler as above, see the RULER CAVEAT) and `python/tools/measure_presets.py` emits it (the default `--variants` are `compressed,tiny,micro,nano,hybrid` plus `mrl256/192/128-int8`, `mrl96-int8`, `mrl256/192/128-int4`):

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

recall@10 here is ANN-vs-exact rank-stability (the ANN index against the exact-cosine top-k on the same queries), NOT real-query retrieval quality, and the printed value mirrors the published tiny ladder number; see the RULER CAVEAT in section 6.

## 9. citations

every search hit carries a stable `citation_id` of the form `nest://<content_hash>/<chunk_id>`. resolve it back to the canonical text and original byte span:

```sh
nest cite my_corpus.nest 'nest://sha256:1aa9.../sha256:8f314...'
```

`content_hash` is hashed over the **decoded** bytes, so a corpus stored with `text_encoding=zstd` produces the same `content_hash` as the same logical content stored raw. citations are stable across wire encodings.

`cite` is tier-1: it returns the stored canonical text plus the verifying hashes (`file_hash`, `content_hash`) and the byte span. it does NOT reopen the original source bytes; original-byte reopen with a blob-digest verify is net-new tier-2 work that belongs to catalog mode, not the flagship. `ask` and `retrieve` print the same tier-1 stored canonical text, so the answer you get is exactly what `cite` resolves.

## 10. release verification

```sh
./scripts/release_check.sh
```

runs the full pipeline: cargo test, clippy, fmt, all 3 python test suites, ruff, `measure_presets.py`, `compare_measure.py` against the committed baseline. exits non-zero on any failure.

## 11. install health check (`nest doctor`)

`doctor` takes no file. it validates the install surface after a one-liner / tarball install (see `doc/install.md`): nest and format versions, the detected simd backend, the python interpreter the embedder will run under, the numpy + tokenizers deps, the potion embedder script, the potion table (a git-lfs pointer is rejected), and one real offline embed of a fixed probe string.

```sh
nest doctor
```

the exit code is typed so installers and ci branch on codes, not text: `0` ok, `2` python interpreter missing, `3` python deps missing, `4` potion embedder script not found, `5` potion table missing or a git-lfs pointer, `6` embedder run failed. a scalar simd fallback prints a warning but still exits `0`. the embedder check opens no socket, so doctor itself stays offline-by-construction.

the embedder script resolves in this order: the repo layout (`python/forge/embed_query_potion.py`, dev checkout), then `${XDG_DATA_HOME:-~/.local/share}/nest/forge/` (one-liner installs), then `<exe>/../share/nest/forge/` (tarball and homebrew-style layouts).

## 12. model registry and multi-model spaces

embedding models are DATA, not per-project code: `python/forge/model_registry.py` holds named presets, each declaring what the model is (ids, dim, the VALIDATED matryoshka ladder), what it needs (deps with the exact pip fix line), and how it is used by default (the asymmetric query/document contract). the build spec (§13) selects one or several presets per build.

| preset | kind | dim | mrl dims | modalities |
|---|---|---|---|---|
| `potion` | static table (offline, no torch) | 256 |; | text |
| `clip-vit-b32` | open_clip ViT-B-32/openai | 512 |; | text, image |
| `siglip2` | open_clip ViT-B-16-SigLIP2/webli | 768 |; | text, image |
| `jina-v5-omni-nano` / `-small` | sentence-transformers | 768 / 1024 | 32–768 / 32–1024 | text, image, video |
| `wemm-2b` | sentence-transformers | 2048 | 128–2048 | text, image, video |
| `wemm-4b` / `wemm-9b` | sentence-transformers | 2560 / 4096 | idem | registered; `--allow-heavy` required |

three rules the registry enforces, loudly:

- **mrl is a ladder, not a slider.** `dims=[256]` is accepted only when the preset's model card validates 256 (`mrl.method="prefix_slice_l2"`). slicing at an unvalidated dim is refused; mathematically possible is not semantically supported.
- **remote code is an opt-in plus a pin.** presets with `trust_remote_code` load only when the spec lists them in `output.allow_remote_code` AND every model-repo code file matches the pinned sha256 allowlist. a hash identifies a version; the opt-in is the consent. build in an isolated environment when the model dir is not fully trusted. the QUERY side has the same rule: a manifest is data, never an authorization, so `ask`/`retrieve`/`search-text` over a remote-code corpus (and `nest_model_bench.py`) refuse to load the model until the operator opts in with `NEST_ALLOW_REMOTE_CODE="<preset>[,<preset>]"` in the environment.
- **three hashes, never conflated.** `model_hash` identifies the model (weights + tokenizer + processor + remote code + pooling/normalize/dtype policy). the per-item `input_hash` identifies the content (canonical text ⊕ image bytes ⊕ label ⊕ chunker). the `embedding_recipe_hash` identifies the usage (prompts, query/document modes, preprocess version, `image_max_side`, device class, decoder fingerprint when embedding decoded media). the embed cache key is the triad, so a retranslated text or a re-exported image invalidates exactly what changed.

known limitation: the siglip2 TEXT tower resolves its hf tokenizer through transformers' AutoTokenizer, which probes optional files that 404 online; a fresh process in strict offline mode can fail that probe even with the snapshot cached. the image tower and every other preset are unaffected; for a sealed offline run either query siglip2 spaces by image, or use the wemm/jina text towers.

model dirs resolve explicit `model_path` > `NEST_MODEL_DIR_<PRESET>` env > the preset's `local_dir` > the hf cache; a hub download requires `NEST_ALLOW_DOWNLOAD=1` explicitly. dtype defaults are measured, not assumed: bf16 on cuda, fp16 on mps (wemm-2b image embeds 0.5s vs 23s in fp32 on this class of machine), fp32 on cpu; override with `dtype=` in the spec or `NEST_ST_DTYPE`.

## 13. declarative corpus builds (`nest build --spec`)

one toml describes the whole corpus; nobody writes a build script per project. `nest build` is a launcher over `python/tools/nest_forge.py` (the build is officially a python frontend; torch and ffmpeg live there).

```sh
nest build --spec corpus.toml --dry-run     # plan + dep status, loads nothing
nest build --spec corpus.toml --sample 1500
```

a complete working spec — a sqlite table with per-row images, two models, media behind the dual quality gate, one self-contained output file:

```toml
[corpus]
name = "cards"
chunker_version = "cards/1"     # changes ⇒ every chunk_id (citation) changes

[source]
kind = "sqlite"
db = "~/data/cards.sqlite"
query = "SELECT id, name, body, image_uri FROM cards WHERE image_uri IS NOT NULL"
order_by = ["id"]               # must be a TOTAL order (verified)

[source.text]
template = """
{name}
{body}
"""

[source.image]
path_template = "~/data/images/{id}.jpg"
label_template = "{name}"

[media]
profile = "stills"              # measured recipe; explicit keys still win
crf = "auto"                    # dual gate: ssimulacra2 strata + embedding drift

[[models]]
preset = "potion"
text = "default"                # space 0 of every emitted file

[[models]]
preset = "clip-vit-b32"
image = "space"                 # a named vector band over the artwork

[output]
mode = "single"
dir = "out/cards"
embed_media = true              # media inlined via 0x17: ONE file serves it all
```

the contract highlights:

- **source**: `sqlite` (query, `[[source.joins]]`, `[source.derive]` helpers, text template whose lines drop when ALL their placeholders are empty, `path_template`/`label_template` for images) | `csv` | `jsonl` | `image_dir`. `order_by` must be a TOTAL order; the composite key's uniqueness is verified against the loaded rows, because `ORDER BY x` with duplicate x is not deterministic.
- **models**: each `[[models]]` names a preset and its role; `text = "default" | "space" | "none"` (exactly ONE default; it is space 0 of every emitted file, never injected implicitly), `image = "space" | "none"`, `dims = [256, 512]` (one named space per dim: `wemm-2b@256`), `space_dtype`, plus the recipe fields (`image_prompt`, `text_query_mode`, `image_max_side`, `encode_kwargs`).
- **media** (§14 for the levers): `profile` (a measured recipe resolved into knob defaults; explicit keys always win), `backend`, `crf` (int or `"auto"`), `tune`, `speed`, `fps`, `gop`, `order`, `shard_size`, `dedup` (identical source images stored once; duplicate rows share the frame through the 0x16 overlay).
- **embedding.image_input**: `mode = "decoded_media"` (default with media: the index describes what the file serves) | `"source"` (measures the model, not the codec). the decoder fingerprint joins the recipe hash in decoded mode; the two modes answer different questions and are never mixed.
- **output**: `mode = "single" | "per-model" | "both"` (one media encode, one embed pass per model, shared across outputs; chunk_ids are content-addressed so citations agree across modes), `provenance = "minimal" | "standard" | "full"` (path/sql/label redaction), `allow_remote_code`, `embed_media = true|false` (inline the encoded media into the `.nest` itself — section 0x17 — so the corpus is ONE self-contained file with no media sidecar at read time; `nest media <file>` lists the blobs, `nest media <file> --export DIR` writes them back out hash-verified, and `nest validate` proves every inlined blob against its `blob_refs` sha256. the sidecar `.media/` dir remains on disk as the build cache; peak build memory is roughly twice the media bytes, so prefer sidecar mode for very large corpora).

every build emits `<name>.manifest.json` (`manifest_schema_version = 1`, canonical serialization; a versioned contract, not an ad-hoc log) and `<name>.build.lock.json` (package versions, tool binaries with sha256, model hashes, the materialized spec). reproduction has three declared levels: L1 = same top-k anywhere; L2 = per-vector cosine within 1e-5 on the same device class; L3 = byte-identical `file_hash`, claimable ONLY under a matching lock (`--rebuild-only` re-emits from the triad-keyed caches and compares the lock; `--strict-env` turns divergence into an error). builds are transactional: per-stage state under `.forge-state/`, outputs staged in `.tmp/` and committed by atomic rename, `--resume` continues from the last intact stage; embed caches are flock'd with checksum sidecars, and a torn cache is recomputed, never reused.

## 14. dataset compression levers and the dual quality gate

the media section is where the compression research became knobs. all decisions land in the manifest and provenance:

- `tune = "still"`: svt-av1's still-picture tune, PROBED against the local encoder (the numeric value varies by version); unsupported ⇒ stderr warning + recorded fallback, never a silently ignored flag. `speed = 6` buys quality per byte over the default 8 at ~2x encode time. `fps` changes playback timestamps only (frames are 1:1 with items; verified, no duplication).
- `crf = "auto"`: the dual gate. a stratified sample (deterministic, versioned bucket heuristics: resolution / entropy / has_text / alpha / source_format) is encoded at every ladder crf and must clear BOTH floors: ssimulacra2 per-bucket p10 ≥ `visual_floor_p10` and global min ≥ `visual_floor_min` (a global average would let one whole stratum degrade), and embedding drift p10 ≥ `drift_floor_p10` measured by the declared `gate_model`; an image can look fine to humans and still move in retrieval space, which is what the corpus actually serves. the largest passing crf wins; none passing ⇒ smallest + a loud warning (measured on the mtgdataset cards: the default floors correctly refused the whole [30,45] ladder; fine card text at 488x680 does not reach p10 ≥ 85). full retrieval recall lives in the sweep, outside this loop, where it costs O(1) per variant.
- `dedup = true`: content-hash dedup of source images; n rows → one frame via the span overlay, zero format change. on the full scryfall printings set the potential is ~48% of the media (100,452 printings, 51,870 unique arts).
- `order = "cluster"`: greedy cosine clustering (deterministic tie-breaks) makes near-duplicates adjacent so per-segment inter coding has something to predict; measured before recommended; on a 1-per-card corpus the honest expectation is ~0, and on the same-artwork reprint corpus it is -29% (2026-08-31, g=16 + scd=0 vs all-intra).
- `gop = "auto"` with sharding probes PER SEGMENT: each `shard_size` chunk runs its own intra-vs-inter probe encode and ships its own keyint (recorded per segment in the manifest, `gop.per_segment = true`). a single global probe averages regimes away — with `order = "cluster"` the near-duplicate runs concentrate in a few segments, which decide inter (bounded gop, keyint=16, scene-change detection off), while unique segments keep O(1) all-intra access. forced `gop = "intra" | "inter"` still applies to every segment alike.
- `profile`: dataset-type presets resolved BEFORE explicit keys (an explicit key always wins, so no other use case is closed off). `"near-dup"` = cluster ordering + per-segment gop + still tune (visually similar corpora: card reprints, video frames, scans); `"stills"` = all-intra + still tune (unique images, O(1) access); `"archive"` = jxl-transcode (byte-reversible, for corpora where loss is not acceptable). the resolved knobs and the profile name both land in the manifest.
- `backend = "jxl"` / `"jxl-transcode"`: the ONLY truly lossless modes. `jxl` is lossless of the source pixels; `jxl-transcode` repacks jpegs reversibly (~20% smaller, round-trip verified by reconstructing the jpeg and comparing sha256). non-transcodable inputs follow `on_unsupported_jpeg = error | copy-source | lossless-jxl`, per-file decisions recorded. preservation contract: decoded pixels (jxl) / original jpeg bytes (verified transcode); exif/icc/xmp only with `keep_metadata`; timestamps and filenames live in the manifest. needs `cjxl`/`djxl` (`brew install jpeg-xl`, which also ships `ssimulacra2` for the gate).

measure everything with `python/tools/nest_image_sweep.py` (variants now include `av1-tune`, `jxl`, `jxl-transcode`) and compare models with the three-tier `python/tools/nest_model_bench.py`: T1 pipeline stability (identity self-retrieval, inflated by construction and labeled as such), T2 codec cost (embedding drift), T3 task utility (label-template text→image as declared weak ground truth, plus `--queries-file` with real operator queries: hit@k, mrr, negative leakage). the tiers answer different questions and are never aggregated into one number.

## 15. media blobs (`nest media`)

a corpus built with `[output] embed_media = true` (§13) carries its encoded media inside the file (section 0x17, an offset table parallel to the `blob_refs` records plus the raw bytes). `nest media` is the read side:

```sh
nest media corpus.nest                 # one line per blob: index, sha256, byte length, inlined or sidecar, original uri
nest media corpus.nest --export DIR    # write every inlined blob to DIR, verifying each against its blob_refs sha256
```

`--export` fails on the first blob whose bytes do not hash to the recorded `content_hash`; `nest validate` performs the same proof over every inlined blob without writing anything. the python side reads one blob without exporting the store: `NestFile.blob_bytes(i)`. the section is content_hash-excluded, so an embedded corpus and its sidecar twin carry the same citations.
