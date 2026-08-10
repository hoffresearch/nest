# changelog

all notable changes to `nest` are documented here.

format follows [keep a changelog](https://keepachangelog.com/en/1.1.0/). versioning follows [semver](https://semver.org/spec/v2.0.0.html). the on-disk container format is frozen at v1; breaking changes bump `NEST_FORMAT_VERSION` (binary) or `NEST_SCHEMA_VERSION` (manifest fields).

## [Unreleased]

### changed

- the offline embedder now resolves its python interpreter in a fixed order instead of a single hard-coded `python3`: an explicit `NEST_PYTHON` wins, else the repo's `.venv/bin/python` discovered by walking up to four ancestors of the cwd, else `python3` on PATH. `nest search-text` previously always spawned `python3` and `ask`/`retrieve` only honored `NEST_PYTHON`, so on a machine whose system `python3` lacks numpy + tokenizers (the potion-table deps) the flagship verbs failed until the user set `NEST_PYTHON` or fixed PATH by hand; the repo `.venv` is now picked up with no extra setup. the embedder opens no socket regardless of interpreter, so the offline-by-construction guarantee is unchanged. centralized in `nest-cli`'s `cmd::pyenv::resolve_interpreter`, with a `resolve_interpreter_from` core unit-tested for the override / `.venv`-discovery / `python3`-fallback precedence. the resolved interpreter is logged to stderr (the choice is never silent), and the ancestor-search trust assumption is documented: with `NEST_PYTHON` unset, discovery executes the nearest ancestor `.venv/bin/python`, so set `NEST_PYTHON` explicitly when running from an untrusted tree.

### added

image and pdf corpus builder (experimental, forge tooling layer only, no format or runtime change):

- `python/forge/embed_image.py` is a vision embedder over `open_clip` (DermLIP for dermatology, a plain architecture plus a pretrained tag for other domains). `model_hash` fingerprints the weights that were actually loaded plus the preprocess transform, not the model name, so the manifest gate can tell two checkpoints apart. a bare architecture name without a pretrained tag is now rejected: open_clip answers that request with RANDOM weights, and a corpus built on random weights searches like noise while looking healthy.
- `python/forge/image_media.py` owns the codec path. every image is letterboxed onto one canvas derived from the dataset's median aspect ratio and median width, with `--width` acting as a ceiling rather than a target so a corpus is never upscaled into encoding interpolated pixels. frames move through a rawvideo pipe rather than the concat demuxer, which re-derives timestamps per input and drops frames it reads as out of order. the encoded frame count is verified against the item count, because a corpus whose `#frame=N` pointers are off by one is worse than no corpus.
- `python/forge/image_items.py` is the discovery layer. an item's position becomes its corpus ordinal, its frame number, and the `byte_start` its citation resolves through, so the ordering is sorted and reproducible, and sampling renumbers densely.
- `python/tools/nest_build_image_corpus.py` emits the `.nest`, one chunk per image or pdf page. precomputed embeddings are handed to `Pipeline` keyed by `chunk_id`, never by position: `Pipeline` passes only the chunks the scratch cache missed, so a positional lookup silently assigns other images' vectors on any partially warm rebuild.
- a corpus is `corpus.nest` beside `corpus.media/`, with `media://<file>#frame=N` uris relative to that pair, so it can be copied to another machine and still resolve. `corpus.manifest.json` records ordinals, origins, labels, pdf page numbers, and the media sha256.
- `python/tools/nest_search_image.py` queries through `retrieve` (so the `model_hash` gate runs), resolves hits back to their original file and page, and can decode the matched frames out of the corpus media.
- `python/tools/nest_image_eval.py` replaces the earlier recall script and reports two rulers separately, never blended. every delta against the control carries a paired percentile bootstrap (5000 resamples, seeded) and a `significant` flag, so a difference the sample cannot resolve is not reported as a finding.
- pdf pages carry their page number into the manifest and into the citable text, and the eval harness re-renders a page from its source pdf when it needs the original pixels, since the build-time renders are temporaries. without that a pdf corpus could be built but never measured.
- `tests/test_image_corpus.py` builds its dataset in-test instead of pointing at a local disk path, and covers frame alignment, the partly-warm cache path, relocatability, the model_hash gate, pdf page provenance, seeded sampling with dense renumbering, letterbox geometry, the no-upscale canvas ceiling, and the bootstrap's refusal to call a sub-resolution difference significant. the vision model is not needed to run it, and `release_check.sh` now runs it.

### note (2026-08-05): the release gate was red before this change, and one stage still is

`release_check.sh` did not run to completion on `main`. it died at the 300-line guard on `crates/nest-runtime/src/ann/codec.rs` (314 lines), so every stage after it was unreachable. splitting that file made the rest of the gate reachable and surfaced two more pre-existing failures: `python/tools/measure_presets.py` failed `ruff format --check` (fixed here, whitespace only, the emitted json is unchanged), and `exact.p95_ms` fails its latency ceiling.

the latency gate compares an absolute p95 in milliseconds against a figure recorded on whatever machine wrote `dat/measure/baseline.json`, so it fails on any slower machine regardless of the code. it was confirmed to be independent of this change: measured on the pre-change binary, `exact` p95 is 6.894 ms; on the post-change binary, 6.827 ms; the ceiling is 4.707 ms. `drift_max` for `exact` is 0.000000, so the scores are bit-identical to the baseline, and all 11 size and recall gates pass. the baseline was NOT re-recorded to make it green, since that would only hide the mismatch. this needs either a machine-relative latency gate or a deliberate re-baseline by the maintainer.

### note (2026-08-05): image corpus ruler provenance

the first draft of this feature reported `recall@10 = 1.00` at ~20x compression on PH2. that number was measured on a self-retrieval ruler: the query image is the source of the very frame being looked for, so the corpus contains the answer and the score mostly reports that the codec did not destroy it. run on the UNCOMPRESSED control the same ruler returns exactly 1.000 at every k, which is what it is worth. it is the same ruler class already flagged for the text ladder above.

measured numbers, PH2 dermoscopy (n=200, 3 classes, DermLIP ViT-B-16, av1 crf 35, canvas 766x576, 1 fps), all 200 images as queries embedded from the ORIGINAL pixels:

- media 74.70 MB of source jpeg to 1.89 MB, 39.5x. the whole shippable corpus, index included, is 2.10 MB, 35.5x.
- identity ruler (self-retrieval, inflated by construction): compressed `recall@1` 0.860, `@5` 0.985, `@10` 0.995, against an uncompressed control that is 1.000 everywhere.
- label ruler (the query's own frame excluded, score is the share of remaining neighbours sharing its diagnosis): compressed `precision@1` 0.695, `@10` 0.576; uncompressed control `precision@1` 0.730, `@10` 0.611; random-pick baseline for this label distribution 0.357.

the finding is the delta, not either column. at 39.5x, av1 costs 3.5 points of label `precision@10`, bootstrap 95 percent CI [-6.1, -1.1] over 5000 paired resamples of the 200 queries, so the cost is real and not a sampling artifact. what remains is still far clear of the random baseline. single dataset, single codec setting, no claim beyond that.

### note (2026-08-07): two corrections to the numbers published two days ago

both were found by controls that should have been run before publishing, and both change what the earlier note claimed.

**the ratio was understated, because the pipeline was upscaling.** `canvas_size` treated `--width` as a target rather than a ceiling, so the 1024 default resized a 765-wide source UP by 34 percent and paid av1 to encode interpolated pixels. `--width` is now a ceiling clamped to the median source width, and nothing else changed:

| canvas | media | ratio | single-frame decode | label `precision@10` |
| ------ | ----- | ----- | ------------------- | -------------------- |
| 1024x768 (upscaled) | 2.89 MB | 25.9x | 124 ms | 0.593 |
| 766x576 (source) | 1.89 MB | 39.5x | 79 ms | 0.576 |

35 percent off the file and 36 percent off random access. the `precision@10` difference between those two rows is 1.7 points with a 95 percent CI of [-3.8, +0.4], so at n=200 it is not distinguishable from noise: the smaller file is not measurably paid for.

**the earlier "av1 costs 1.9 points of `precision@10`" was not a supported claim.** that delta's CI is [-4.2, +0.5] and crosses zero. at 25.9x the codec's cost was simply below what 200 queries can resolve, and reporting a point estimate without an interval implied a precision the experiment did not have. the cost only becomes measurable once the ratio goes past roughly 26x:

| setting | ratio | label `precision@10` | delta vs uncompressed control | verdict |
| ------- | ----- | -------------------- | ----------------------------- | ------- |
| uncompressed control | 1.0x | 0.611 | - | - |
| 1024 canvas, gop 161 | 25.9x | 0.593 | -1.8 pts, CI [-4.2, +0.5] | within noise |
| source canvas, gop 161 | 39.5x | 0.576 | -3.5 pts, CI [-6.1, -1.1] | real |
| source canvas, all-intra | 44.7x | 0.570 | -4.1 pts, CI [-7.0, -1.1] | real |

`--all-intra` (`keyint=1`) stays a lever, default off. it follows from the upstream finding that reordering images changes the ratio by only ~6 percent, so the gain is intra-frame: unrelated images give motion estimation nothing to find and the bits it spends searching are wasted. it buys another 5.2x for 0.6 points of `precision@10`, a difference well inside the interval, and every frame becomes a keyframe so random access has nothing to seek back to.

### changed (2026-08-10, supersedes one line of the 2026-08-07 note)

- the gop is no longer a flag the caller must guess. `gop_policy="auto"` is the builder default: an evenly spaced sample (32 frames) is probe-encoded both ways at the target crf and the smaller stream wins, because fase 0 measured that embedding cosine does not separate intra-favouring from inter-favouring corpora, so the policy is a measured encode decision. the probe statistics (n_samples, intra_bytes, inter_bytes, decision) land in the manifest next to the toolchain provenance. `--gop-policy intra|inter` forces a side, and `--all-intra` keeps working as the explicit intra override. the 2026-08-07 note's "lever, default off" described the flag era.
- `build_corpus` moved from `python/tools/nest_build_image_corpus.py` to `python/forge/image_corpus.py` (the tool is a thin cli re-exporting it), keeping every first-party module under 300 lines.

### added (2026-08-10): media blob sections, multimodal space bands, scale levers (all additive within frozen format v1, all excluded from content_hash, so a corpus with any of them keeps the citations of its text-only twin)

- `0x14 blob_refs` + `0x16 blob_span_overlay` (F4b), the self-contained single-file layout for media corpora. the codecs raise typed errors on every truncation with hostile-count bounds before any allocation. both join `OPTIONAL_SECTIONS` behind the new additive `capabilities_ext.blobs_present` flag. the overlay rewrites blob-pointing spans in place at open, so `cite` and `retrieve` report real blob-relative byte ranges instead of the 0x03 ordinal placeholders; a dangling blob_ref_index fails open with a typed error. `nest.build(blob_refs=..., chunk_blob_spans=...)`, `NestFile.has_blobs`, `NestFile.blob_refs()`, and `inspect` lists the blob table.
- `0x15 space_table` + per-space vector bands `0x20-0x2F` (fp sources `0x30-0x3F`) (F4c): text and vision vectors in the same frozen file, each space with its own `model_hash` and dim, behind `capabilities_ext.supports_multimodal`. bands follow the embeddings encoding rule (dtype encodings only, never zstd) and the reader validates every listed band against its (n_vectors, dim, dtype). `search_space(name, query, k, expected_model_hash)` scores the band with the shared exact-cosine kernel; the text paths never read a band and the space path never reads 0x04, so a text query can never be scored against vision by accident (isolation test). `nest.build(spaces=[(name, model_hash, dtype, vectors)])`, `NestFile.search_space` / `has_spaces` / `space_names`.
- fase 2 pipeline hardening (forge tooling layer): seek-based random access (`decode_frame`, `decode_frames_at`: one ffmpeg invocation resolves k ordinals; measured 42-67 ms against the legacy select scan's 97-189 ms, byte-identical to the sequential path, asserted on both gop and all-intra media); toolchain provenance (`toolchain` + `provenance_sha256` on every encode, because another encoder version produces other pixels and an index that silently stops matching the media); per-frame `sha256` recorded in the manifest with `verify_frame_hashes`, which catches a REORDERED stream the frame-count guard cannot see; `pix_fmt` probed after every encode (this libsvtav1 converts 444 to 420 silently, and the lie now raises); symmetric letterbox preprocessing on queries (`--letterbox-query`); text queries through the clip text tower (`--query-text`); the avif backend (per-image O(1) semantics and real yuv444, the chroma the measured melanoma breakdown asks for on medical corpora); the letterbox-lossless control build mode (`--control`), the ruler every codec cost is measured against.
- fase 3 measurement harness: `python/tools/_image_metrics.py` (paired bootstrap delta with ci95, sign test, per-class floor, cosine drift, kendall tau-b plus overlap) and `python/tools/nest_image_sweep.py` (one model load, control first, the whole variant matrix in-process, every delta against the SAME control with an interval, a sign test, and the per-class floor; the mean is not the gate, the worst class is). the control now records its own byte size and every variant reports `nest_bytes`, because the per-backend `compression_ratio` was never the same unit (av1 sums original files, avif sums its letterboxed png inputs) and the dtype ladder only moves the index.
- fase 5 scale levers (forge tooling layer): the gop probe above; `--shard-size` splits the av1 stream into consecutive ~1000-frame segments with a segment index in the manifest (uris name segment plus local frame, so the read side resolves unchanged; true append with index merge is NOT implemented and is declared as such); `--order-similarity` permutes frames by a greedy nearest-neighbour walk before encode, with uris, vectors, and frame hashes un-permuted back to item order (the tests decode every uri and compare content hashes to prove it); `--dtype` overrides the preset's vector dtype, and the sweep gains `dtype:f32/f16/int8/int4` variants that ride on the preset base to isolate quantization, plus an `av1-order` kind that pins inter (ordering is byte-invisible under all-intra). caveat recorded: dermlip is not mrl-trained, so prefix truncation should degrade more than on an mrl model; pca is the documented fallback if int4/int8 collapse.

### note (2026-08-10): the full matrix, measured with intervals (fase 6)

every number below comes from the shipped sweep (`python/tools/nest_image_sweep.py`) against the letterbox-lossless control, and every delta carries a paired bootstrap ci95; json artifacts under `tmp/fase6/` (not tracked). "vs control" sizes use the control's own media bytes, the only ruler the variants share.

- ph2 dermoscopy (n=200, dermlip vit-b-16, canvas 766x576, 100 queries, control media 144.0 MB, control label precision@10 0.621, random baseline 0.357): av1 intra crf35 1.67 MB (86x vs control), crf40 1.20 MB (120x); mean precision@10 deltas -3.4 to -4.7 pts with CIs that mostly cross zero at this n. the class floor does not cross: melanoma -16.9 pts CI [-25.0, -10.0] at crf35 yuv420, significant, while both nevus classes are flat. avif444 at the same rate mitigates to -12.5 [-18.8, -6.3]; vector quantization over the same decoded frames is not the driver (melanoma -21.3 on f16, -21.3 on int8, -24.4 on int4, overlapping intervals). the mean said "about -3.5 pts"; the floor says a diagnostic class pays six times that.
- ham10000 (sample 2000 seed 42 of 10015, 200 queries, canvas 600x450, control media 801.0 MB): av1 intra crf35 5.31 MB (0.66 percent of the control, 151x). mean label precision@10 delta -1.5 pts CI [-3.5, +0.6], crossing zero. per class: nv +1.9 (n=149, not significant), mel -10.7 CI [-15.7, -6.4] (n=14, significant), bkl -13.3 CI [-24.4, -2.2] (significant), vasc -40.0 (n=3, floor trip). int8 quantization repeats the same deltas to the first decimal. the per-class n is small by construction of the sample; the intervals are the honest width of that.
- text-tower ruler (6 clinical queries through dermlip's text tower, target-label hits in top-10, base-rate expectation ~6.3/60): control 44/60, crf35 22/60, crf30 26/60. text queries beat the base rate at every rate point (3.5x at crf35), but the codec cost on the text ruler dwarfs the image ruler's and is NOT recovered by spending 56 percent more bytes at crf 30: lossy pixel perturbation shuffles tight cross-modal margins instead of being rate-limited. a corpus whose primary use is clinical text search should budget for that gap or stay lossless; this is now a documented limitation with numbers, not an impression.
- wsi tiles (n=1210 in scan order, canvas 512x512, control media 612.9 MB): intra 31.9 MB (5.2 percent), inter 34.4 MB (5.6 percent); the probe picks intra. similarity ordering on the inter stream: -0.15 percent (slightly worse). the fase 0 +6.6 percent ordering figure does not transfer even to scan-ordered material, so the flag stays opt-in and the novelty claim below drops "ordering" entirely.
- pdf pages (508 pages of a 1907 public-domain bird book, canvas 1024x1464, control media 166.2 MB): intra 37.1 MB (22.3 percent), drift median 0.983. same codec, same crf, and the ratio collapses from 151x (dermoscopy) to 19x (tiles) to 4.5x (book pages): the compression ratio is a property of the domain, not of the codec.
- the gop probe's decisions on real corpora agree with fase 0: intra won on ph2, ham10000, wsi tiles, and pdf pages, and the probe said so on bytes in every build above.

### note (2026-08-10): prior art, verified against primary sources, and the novelty claim with its refutations

no citation from memory; every reference below was re-verified against its primary source on 2026-08-10.

what is NOT new, and who did it first: storing an image collection as a video stream is lerobot's format for robot demonstrations (mp4 for visual observations, parquet for tabular; cadene et al. 2024, described e.g. in arXiv 2503.14734 appendix d) and xarrayvideo's for earth-system cubes (pellicer-valero, aybar, camps-valls, "video compression for spatiotemporal earth system data", arXiv 2506.19656, reporting 20-50x). per-image av1-in-heif is avif (aomedia av1-avif spec v1.1.0; han et al., "a technical overview of av1", arXiv 2008.06091). the neighbouring codec sciences are jpeg xl (iso/iec 18181; sneyers et al., arXiv 2506.05987) and the neural codecs dcvc (li, li, lu, neurips 2021, arXiv 2109.15047) and elic (he et al., cvpr 2022, pp. 5718-5727). vector search at scale is faiss (douze et al., arXiv 2401.08281). lossy compression in clinical pipelines is governed by the daic line (acr-aapm-siim technical standard for electronic practice of medical imaging, 2017 revision; the esr position paper on irreversible compression, insights into imaging 2011), which is exactly why the per-class floor exists: "diagnostically acceptable" is a per-task property, and a mean cannot show it.

refutations, attached so the claim cannot grow later: (1) ordering frames by visual similarity does not multiply compression (+6.6 percent on unrelated images in fase 0, -0.15 percent on scan-ordered wsi tiles in fase 6). (2) av1 over EMBEDDINGS is catastrophic, not clever: recall@1 97 percent to 5.9 (fase 0), so the vectors are embedded from decoded frames, never compressed themselves. (3) the headline ratio is a property of the domain, not the codec: 4.5x to 151x at identical settings across the three fase 6 domains. (4) the text-tower gap above: codec-backed corpora answer text queries well above chance but measurably worse than their lossless control at crf 30-35.

the claim that survives verification: nest is, to our knowledge and checked against the sources above, the only system that combines a codec-backed corpus with citation-grade provenance (four hashes, per-frame content hashes, toolchain fingerprint), a per-space model_hash gate with hard text/vision isolation inside one frozen single-file format, two-ruler measurement where every published delta carries an interval and a per-class floor, and a build-time policy probe that decides the gop on measured bytes. each element exists elsewhere; the combination inside one hash-verified file does not, per the verified sources.

### test surface (2026-08-10)

- 340 rust tests in the sovereign workspace (`cargo test --release --workspace`; was 288 in v0.3.0), including the blob_refs and space_table roundtrips, their negative fuzz suites, reserved_ids band disjointness, the runtime blob overlay and space isolation cases, and the content_hash-equality assertions that keep citations stable.
- python: `tests/test_image_corpus.py` at 37 cases (was 15 in v0.3.0), plus `tests/test_blob_bridge.py` (5) and `tests/test_space_bridge.py` (5), all run by `release_check.sh`; the full gate is green on this change.

## [0.3.0] - 2026-06-10

for the codec-vs-codec question, at matched source resolution and measured on the same 200 images: source jpeg 74.70 MB (6.8 bits/pixel, stored near-lossless), a fair jpeg q95 re-encode 31.87 MB, webp q90 19.44 MB, jpeg q85 16.20 MB, av1 crf35 1.94 MB. so av1 is ~16x smaller than a fair jpeg q95 baseline, and roughly 2.3x of the headline 39.5x is the source having been stored wastefully rather than anything the codec did.


## [0.3.0] - 2026-06-10

additive release within frozen format v1. extends v0.2.0 with the int4 sub-int8 lever and the published preset ladder, the g1 graph pillar, matryoshka prefix truncation, and the forge-core ingestion workspace. existing v0.2 files load unchanged in v0.3 readers.

### note (2026-06-07): recall ruler provenance

every `recall@10` figure in this changelog is measured on a SELF-PERTURBATION ruler (each query is a corpus vector plus tiny noise, a near-duplicate of an existing point), so it reports rank-stability under quantization, NOT real-query retrieval, and is likely inflated. `dat/measure/ladder.json` and `dat/measure/baseline.json` now carry a `ruler` provenance field saying so. the real-query (mteb-style) ruler is gate-zero (pending).

### added

int4 block-64 embeddings, the first real sub-int8 size lever (additive within v1, no format-version bump):

- `encoding=7` int4 embeddings. layout: 8-byte prefix (`payload_version=1`, `scale_kind=1`), then per-64-dim-group f16 absmax scales (row-major), then packed 4-bit signed codes (`[-7, 7]`, two nibbles per byte, low nibble first). requires `dtype="int4"` and `embedding_dim` divisible by 64. each 64-dim block carries its own scale so one block's outlier cannot crush another. validated like int8: the 4-bit codes cannot encode NaN/Inf, only the f16 group scales are range-checked.
- fused dequant+dot kernel in the runtime simd module (`dot_f32_i4_blocked`): avx2 and neon vectorize the nibble unpack 16 packed bytes at a time, then run the identical per-group scalar reduction, so all three backends agree bit-for-bit (a lane-parallel float reduction would diverge in the last ulp). the embeddings section is scored straight off mmap; like int8 it is never zstd/dedup/shuffled.
- the mandatory exact rerank reads the stored int4 slab (no separate full-precision source, exactly like int8), so the returned `score` is real cosine AT THE INT4 STORED PRECISION. disclosed via `manifest.dtype` and the `dtype=int4` / `encoding=int4` lines in `nest stats`, never reported as a bare-slab ratio.
- `nano` preset: zstd text, int4 embeddings, HNSW, no BM25. sits below `tiny`. on the project's PT-BR corpus (n=30,725, dim=384, NEON, 100 queries, k=10): embeddings section 11.92 MB (int8) -> 6.27 MB (int4), ~1.9x over int8 and ~7.5x over float32; file size_ratio 0.209; recall@10 0.913 vs the float32 exact baseline.
- `dtype` extended to `"float32" | "float16" | "int8" | "int4"`. python `build(..., preset="nano")` or `dtype="int4"` rejects `embedding_dim` not divisible by 64 with a typed error.
- `compare_measure.py` gains conditional `nano` regression gates (`size_ratio <= 0.25`, `recall_at_k >= 0.85`), active only when the run includes the nano preset.

scope note: rabitq (encoding 8), dedup-before-zstd (0x0B), and chunk_scalars (0x0D) are the other legs of the same plan task and are not part of this change.

published preset ladder + honest net-of-fp reconciliation (measurement and docs only, no format/runtime change):

- `measure_presets.py` names two explicit sub-int8 ladder rungs: `nano` (int4 full-dim) and `micro` (the matryoshka lever, an alias for `mrl256-int8`: `mrl_dim=256` + int8). the default `--variants` now emit the full ladder `compressed,tiny,micro,nano,hybrid` plus the mrl curve `mrl256/192/128-int8`, `mrl96-int8`, `mrl256/192/128-int4`.
- the full ladder ran at 100 queries, k=10, on the LFS baseline `dat/corpus_next.v1.nest` (n=30,725, dim=384, NEON). published to the new `dat/measure/ladder.json` (the curve, not a cherry-pick) and `dat/measure/baseline.json`'s named rows refreshed to the freshly-measured numbers.
- honest reconciliation: the intpack chunk_ids/spans repack and the bitpacked hnsw/bm25 payloads shrank the indexed presets below the v0.2 published figures. the committed numbers now match the build: `tiny` 0.283 -> 0.256, `compressed` 0.350 -> 0.339, `hybrid` 0.668 -> 0.609. recall@10 unchanged (`tiny` 0.992, `compressed`/`hybrid` 1.000).
- net-of-fp framing: the 0x09 `embeddings_fp` writer is not wired, so every shipped sub-int8 preset (`nano`/int4, `micro`/mrl-int8, the mrl-int4 curve) is STORED-PRECISION: net-of-fp ratio == stored ratio, disclosed as real cosine at the stored int4/int8 precision. each published sub-int8 row carries `dtype`, `stored_precision`, `has_fp_source=false`, `net_of_fp_ratio`, and (for matryoshka rows) `mrl_dim`/`full_dim`, so the disclosure is machine-checkable. rabitq 1-bit (which would need a counted f16 fp source) stays out of scope.
- `compare_measure.py` gains a conditional `micro` gate (`size_ratio <= 0.25`, `recall_at_k >= 0.78`), mirroring the existing `nano` and `mrl256-int8` gates; it fires only when the run includes `micro`.
- published numbers (100 queries, k=10, vs the float32 exact baseline): `compressed` 0.339 / 1.000, `tiny` 0.256 / 0.992, `micro` 0.223 / 0.810, `nano` 0.209 / 0.913, `hybrid` 0.609 / 1.000.

g1 graph pillar: chunk-to-chunk graph adjacency (section 0x0C) + `search-graph` (additive within v1, no format-version bump):

- new optional section `0x0C graph_adjacency`: a chunk-to-chunk csr with typed edges (`NEXT_CHUNK=0`, `SEMANTIC=1`, `CITATION=2`). the payload is a self-describing raw section reusing intpack internally: `GRAPH_ADJACENCY_PAYLOAD_VERSION=1`, n_nodes, intpack csr offsets, per-(src,edge_type)-run delta-gapped neighbor ids, and a falkordb-iso edge-type column (a single scalar when uniform). canonical ascending (src, edge_type, dst) sort makes two builds of the same edge set byte-identical.
- excluded from `content_hash` (not in `CANONICAL_SECTIONS`), so adding a graph never invalidates a `nest://` citation. gated by the additive `capabilities_ext.graph_present` flag and opened in `mmap_file` behind it.
- runtime `graph::CsrIndex` parses the section off mmap into flat offsets plus neighbors (one node's neighbors are a contiguous zero-alloc slice); `graph::Traversal::bounded_bfs` walks it with a generational visited buffer (no per-query HashSet), bounded by hops and max_frontier.
- 9th engine subcommand `nest search-graph <file> <qvec> -k K --hops N --ef N`: seeds the exact-cosine top-ef, bfs-expands the union, and feeds it to the SAME `score_subset` exact rerank (`index_type=graph`, `recall=NaN`; the candidate-generator contract is asserted in `rerank_contract.rs`). falls back to exact when no 0x0C section is present.
- build side: `nest.build(with_graph=True, graph_top_m=...)` emits NEXT_CHUNK (sequential ordinals, both directions) plus top-m SEMANTIC edges from the already-built hnsw level-0 graph. `drop_overlap` implies `with_graph`; read-side neighbor context via `python/graph_context.py`; recall gate in `python/tools/graph_recall_gate.py`.

matryoshka prefix truncation (`mrl_dim`), the build-time dimension lever (additive within v1):

- `nest.build(mrl_dim=K)` slices each l2-normalized row to its first K components and re-l2-normalizes the prefix BEFORE quantization and hnsw, so int8/int4 calibrate on the shorter renormalized row. orthogonal to and multiplicative with the dtype levers.
- additive optional manifest pair `mrl_dim`/`full_dim` (both omitted when unset, so existing files stay byte-identical); the stored `embedding_dim` becomes K, the source dim is recorded as `full_dim`, both shown in `nest stats`.
- NO runtime kernel change: the reader strides by `header.embedding_dim`. int4 needs the effective dim divisible by 64, so the int4 ladder is valid only at `mrl_dim` in {256, 192, 128}.
- `content_hash` is over the truncated embeddings, so a citation is tied to its `mrl_dim`, never claimed stable across dims. truncation is a pure deterministic slice, so builds stay byte-identical; guarded by `tests/mrl_truncate.rs`.
- on the shipped non-mrl MiniLM baseline truncation costs real recall@10, published as a curve (same self-perturbation ruler caveat as the note above); `mrl256-int8` is the named `micro` preset.

forge-core (FORGE-0a): the ingestion layer's frozen .fci schema, in a separate workspace:

- new cargo workspace `forge-core/` at the repo root, deliberately OUTSIDE `crates/`, so its dependency tree never enters the sovereign crates; not built by the sovereign release gate. build, test, and lint it via `--manifest-path forge-core/Cargo.toml`.
- FORGE-0a ships the frozen .fci canonical-intermediate schema only: `FciBundle`, `ChunkRecord` (mirroring `builder.ChunkSpec` 1:1 so spans round-trip through `nest cite`), `EmbeddingRequest`/`SpaceTag`/`PayloadRef`, `Entity`/`MentionSpan`/`Edge`, and `BlobRef`.
- `FCI_SCHEMA_VERSION` is versioned independently of `NEST_FORMAT_VERSION`. serialization is deterministic canonical compact json (declaration order, verbatim strings), covered by roundtrip and determinism tests.

### test surface

- 288 rust tests in the sovereign workspace (`cargo test --release --workspace`, 35 suites; was 134 in v0.2.0), plus 6 forge-core tests on its own manifest (`cargo test --manifest-path forge-core/Cargo.toml`).
- new groups since v0.2.0: txt_streams roundtrip plus negatives, zstd_dict roundtrip plus negatives, fsst roundtrip plus negatives, dedup roundtrip plus order-invariant, content_hash_dict_fsst_dedup, graph_adjacency roundtrip plus negatives, int4 roundtrip plus negatives, mrl_truncate, manifest_additivity, reserved_ids, the expanded rerank_contract (graph path, SearchExplain, stored-precision disclosure), and forge-core serialize.
- python: the 4 test scripts run by `release_check.sh` (`test_e2e.py` incl the flagship retrieve guard, `test_builder.py`, `test_search_text_model_hash.py`, `test_image_corpus.py` at 15 cases) plus the self-test scripts under `python/forge/` (potion, lexical floor, retrieve), which are not.

### compatibility

no format break. the new optional sections (0x0A dictionary, 0x0B dedup_map, 0x0C graph_adjacency), wire encodings (4 intpack, 5 zstd_dict, 7 int4, 9 fsst, 10 txt_streams), and additive manifest fields (mrl_dim/full_dim, capabilities_ext) all live within v1. v0.2 readers skip the unknown optional sections and reject the new encodings with a typed error, never silently. files built without the new features stay byte-identical.

[0.3.0]: https://github.com/hoffresearch/nest/releases/tag/v0.3.0

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
- `nest search-text <file> "query" -k K [--model-path PATH] [--skip-model-hash-check]`: shells out to `python/embed_query.py`, validates the embedder's `embedding_model`, `embedding_dim`, and `model_hash` against the manifest, then routes to the declared `index_type` (exact, hnsw, hybrid). a model mismatch fails with a typed error, never silently.
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
- file hygiene cap: every rust source file in `crates/**/src/**` and every first-party python module is at most 300 lines. test files exempt.

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
- `model_hash`: caller-supplied; format `sha256:<64 hex chars>` enforced at write time. v0.1 accepted any value matching the regex; v0.2 enforces a real fingerprint.

### manifest contract

JCS-style canonical JSON (declaration-ordered known fields, BTreeMap order for `extra`, no whitespace). required values for v0.1:

- `dtype = "float32"`, `metric = "ip"`, `score_type = "cosine"`, `normalize = "l2"`, `index_type = "exact"`, `rerank_policy = "none"`.
- `capabilities.supports_exact = true`, `capabilities.supports_reproducible_build = true`.
- `model_hash` matches `sha256:<64 hex>` regex.

### reproducibility

- `NestFileBuilder::reproducible(true)` overrides `manifest.created` to `"1970-01-01T00:00:00Z"` (`REPRODUCIBLE_CREATED`).
- two builds with identical inputs produce byte-identical files. verified on the legacy converter (`dat/truw_ptbr.nest` to 73.73 MB v0.1 binary); both builds shasum to `b9f6e0ea16176706f08767559927737ce91070147ec6cb54e26710bff3d2566d`.

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
- legacy SQLite-based dataset: `dat/truw_ptbr.nest` (28 MB) to `dat/truw_ptbr.v1.nest` (73.73 MB, 19,769 chunks dim 384) via `python/convert_legacy.py`.
- architecture references: `doc/arc/arc.md` and `doc/arc/arc.yaml`.

[0.1.0]: https://github.com/hoffresearch/nest/releases/tag/v0.1.0
