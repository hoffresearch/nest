---
project: nest
last-updated: "2026-06-06"
domain: architecture
---

# architecture inventory

this file is the human readable companion to doc/arc/arc.yaml and doc/arc/arc.mmd. it is the single human architecture reference in this directory and records what each requested path does and how it fits into the repo.

the trio is kept in sync: arc.md (human), arc.yaml (machine), arc.mmd (visual). any change to boundaries, data flow, sections, or runtime contract must update all three in the same commit.

## system view

- nest-format is the frozen v1 container contract. it owns layout, manifest, reader, writer, canonical sections, hashes, and wire encodings. embeddings stay raw float32, float16, int8, or int4 (fixed-stride quant only); zstd is reserved for non-embedding sections.
- nest-runtime opens .nest files through mmap, validates queries, dispatches SIMD, and executes exact, ann, and hybrid search while reranking candidates with real cosine. that hot path is why embeddings are never zstd-compressed.
- nest-cli is a thin clap surface over the runtime and format crates. each command does argument handling and output formatting, not core search math.
- nest-python is the PyO3 bridge. python/nest.py loads the compiled extension, python/builder.py orchestrates chunking and caching, and python/embed_query.py preserves model parity for search-text.
- scripts/release_check.sh is the release gate. it rebuilds the extension, runs rust and python suites, checks lint and format, measures presets, and compares against the committed baseline.
- forge-core is the ingestion layer, a SEPARATE cargo workspace at the repo root OUTSIDE crates/, so its (eventually heavy, possibly non-deterministic) deps never enter nest-format or nest-runtime. this phase ships FORGE-0a: the frozen .fci canonical-intermediate schema only. forge never duplicates the one authoritative chunker (builder.chunk_text), and .fci is versioned independently of the .nest format. the sovereign release gate does not build it; it has its own cargo workspace.
- python/forge is the forge python surface. its build-side default is now an OFFLINE SEMANTIC static embedder (python/forge/embed_potion.py): a vendored model2vec/potion-base-8M table (mit, dim 256, ~30mb via git-lfs) whose token rows carry distilled meaning, so synonyms cluster (car ~ automobile >> car ~ banana) with no torch, no model download, and no network round-trip. inference is numpy + tokenizers only (tokenize, gather token rows, mean pool, l2 normalize), matching the model2vec reference vector-for-vector. it is offline by construction (hf offline flags forced, no socket at embed time) and self-fingerprints to a sha256 model_hash over the vendored files, recorded in provenance, so builds are byte-identical and a query embedder that disagrees fails the manifest model_hash gate. the #04 lexical bag-of-words (python/forge/embed_default.py) stays as the stdlib-only zero-dependency FLOOR/fallback; a power user can still bring a stronger sentence-transformers model as the quality ceiling. a cc0 demo corpus (python/forge/demo_corpus) ships beside it for the one-gif demo.

## format and runtime contract

- required sections are chunk_ids, chunks_canonical, chunks_original_spans, embeddings, provenance, and search_contract. optional sections are hnsw_index and bm25_index.
- additive section ids past the implemented 0x01-0x08 are reserved in one reconciled, disjoint map in layout/mod.rs: 0x09-0x10 (scalar reservations: embeddings_fp, dictionary, dedup_map, graph_adjacency, chunk_scalars, tokenizer_model, edit_journal, repro_manifest), 0x11-0x16 (graph_nodes, graph_edge_props, graph_entity_map, blob_refs, space_table, blob_span_overlay), and the per-space bands 0x20-0x2F (space embeddings) and 0x30-0x3F (space fp). all are excluded from content_hash and unresolved by section_name until their feature ships; tests/reserved_ids.rs asserts the bands are disjoint and content_hash-safe.
- wire encodings reserve ids 4-10 (intpack, zstd_dict, frontcode, int4, rabitq, fsst, txt_streams); decode_payload dispatches through a wire-codec registry that still rejects the reserved-but-unimplemented ids (zstd_dict 5, frontcode 6, rabitq 8, fsst 9). int4 (id 7) is implemented: a block-64 per-group quantized embeddings section (8-byte prefix, then per-64-dim-group f16 absmax scales, then packed 4-bit signed codes in [-7, 7], two nibbles per byte), requiring dtype="int4" and embedding_dim divisible by 64. it is the first real sub-int8 size lever: the embeddings section drops from int8's ~1 byte/dim to ~0.5 byte/dim + a small f16 scale per 64 dims (~1.9x over int8, ~7.5x over float32). the runtime scores it straight off mmap with a fused dequant+dot kernel (avx2/neon/scalar, bit-for-bit equal across backends since the per-group reduction stays scalar); like int8 it is never zstd/dedup/shuffled. intpack (id 4) is implemented: a content_hash-preserving repack of the canonical chunk_ids and spans sections. under a compressed (zstd-text) preset the writer stores chunk_ids as 32 raw digest bytes and spans as a deduped uri pool plus bitpacked offsets; both decode BYTE-IDENTICALLY to the raw payload, so content_hash and nest:// citations are unchanged. raw-text presets (and the golden fixture) keep the raw/zstd encodings and stay byte-identical. the shared intpack primitive (per-128-block frame-of-reference, O(1) select, never panics on truncated input) also backs the optional indices: the hnsw (0x07) and bm25 (0x08) payloads bumped to version 2, bitpacking neighbour ids and delta-gapped postings; readers still accept version 1, and the decoded graph/index is identical so recall and scores are unchanged. these are optional/non-content-hashed sections, so the bump is additive within v1. txt_streams (id 10) is implemented: a content_hash-preserving re-layout of the chunks_canonical (0x02) section's COMPRESSED form from one concatenated zstd-19 blob into N independently zstd-encoded streams (one per canonical string) behind an intpack offset table that gives O(1) single-chunk seek/reopen. it decodes BYTE-IDENTICALLY to the raw chunks_canonical payload, so content_hash and citations are unchanged. under a compressed preset the writer takes the SMALLER of single-frame zstd vs txt_streams (the same try-smaller pattern as spans intpack-vs-zstd), so the build never regresses; on the shipped many-short-similar-chunks corpus the per-chunk frame loses cross-chunk LZ context (+85.9% on the text section, 15.73MB -> 29.25MB), so single-frame zstd wins and the file stays byte-identical. the value is the layout: it is the named prerequisite for the dict(5)/fsst(9) text levers (where a trained dict/fsst can beat one big zstd-19 blob) and the on-disk O(1) single-chunk reopen enabler (the runtime per-chunk accessor for cite/materialize is a follow-on, out of this scope). raw-text presets (and the golden fixture) stay raw, never txt_streams.
- raw and zstd apply to non-embedding payloads, plus the two content_hash-preserving repacks: intpack (chunk_ids/spans) and txt_streams (chunks_canonical). float16, int8, and int4 apply only to embeddings. embeddings never use zstd because the runtime scores them directly from mmap through the simd path.
- matryoshka prefix truncation is a build-time dimension lever orthogonal to and multiplicative with value precision (Qwen3/ST/BGE truncate-then-renormalize). nest.build(mrl_dim=K) slices each l2-normalized f32 row to its first K components and re-l2-normalizes on the prefix BEFORE quantization, so int8/int4 calibrate and the hnsw graph builds on the shorter renormalized row. the header/manifest embedding_dim becomes K and the source dim is recorded as full_dim; mrl_dim+full_dim ride the manifest as additive optional fields. NO runtime kernel change: the reader strides embeddings exclusively by header.embedding_dim, and the avx2/neon/scalar dot kernels and the int8/int4 views run unchanged on the shorter stride. exact rerank stays true cosine because the prefix is renormalized. it is a pure deterministic slice + renorm, so same chunks + same model fingerprint + same mrl_dim => byte-identical file. content_hash is over the decoded embeddings bytes, so a truncated file legitimately differs from full-dim: citations are tied to a given mrl_dim, never claimed stable across dims (same caveat as the quantized presets). int4 still needs the EFFECTIVE dim divisible by 64, so the multiplicative int4 ladder is valid only at mrl_dim in {256,192,128} (96 is blocked). on the shipped MiniLM baseline (NOT mrl-trained), truncation costs real recall (see measure_presets.py mrl ladder + the WO result note); it is reported as a measured recall@10/size curve per the compression-honesty contract, not a cherry-pick.
- the four integrity surfaces are header_checksum, section checksum over physical bytes, file_hash over the whole pre-footer body, and content_hash over decoded canonical sections.
- the manifest is covered by file_hash, never content_hash. new manifest fields must be additive: Option with skip_serializing_if, so an unset field serializes to nothing (existing files stay byte-identical and old readers still deserialize). the matryoshka disclosure pair (mrl_dim, full_dim) follows this rule: both Option<u32>, omitted when unset, and validated for internal consistency (0 < mrl_dim == embedding_dim <= full_dim, both-or-neither, int4 needs mrl_dim%64==0). new capability flags go in the optional capabilities_ext or the flattened extra map, never as new required bools on Capabilities (which would break both deserialization and file_hash). tests/manifest_additivity.rs is the guard.
- exact search is always the ground truth path. hnsw and hybrid search return final hits only after exact cosine rerank.
- the mandatory exact rerank reads through one explicit rerank source (runtime/src/rerank.rs): the full-precision embeddings_fp (0x09) slab when present, else the stored dtype slab, so the returned score is always real cosine (at stored precision unless an fp source is present). int4 follows int8 here exactly: no separate fp source, the rerank reads the stored int4 slab, so the score is real cosine AT THE INT4 STORED PRECISION, disclosed via manifest.dtype (and the dtype/encoding=int4 line in nest stats and SearchHit.score_type=cosine), not a bare-slab claim. every candidate-generating path (ann, hybrid, and future graph/space/cross) ends in this source; the rerank-honesty contract test (runtime/tests/rerank_contract.rs) asserts the returned score equals the exact rerank byte-for-byte and recall is NaN for every non-exact path, and gates the release check.
- the optional embeddings_fp (0x09) section is a fixed-stride raw slab (float32 or float16, never zstd) read at open; its write path lands with the sub-int8 codecs (phase 3).
- citation ids use the form nest://content_hash/chunk_id, so references remain stable across raw versus zstd text encodings.

## root

- .editorconfig | config | pins utf-8, lf, indentation, and binary-safe editing rules for .nest fixtures.
- .gitattributes | config | routes large binary assets through git lfs and keeps golden fixtures in normal git.
- .gitignore | config | excludes build outputs, caches, virtualenvs, regenerated corpora, and local scratch paths.
- AGENTS.md | repo operations | defines build commands, file hygiene, architectural conventions, and known runtime gotchas.
- Cargo.lock | lockfile | generated dependency snapshot for reproducible cargo resolution, with no business logic of its own.
- Cargo.toml | workspace manifest | declares the four crates, resolver 3, workspace edition 2024, and shared dependencies.
- clippy.toml | lint policy | fixes msrv and lint thresholds so the release gate behaves deterministically across clippy updates.
- CODE_OF_CONDUCT.md | governance | contributor conduct policy in the repo's lowercase documentation style.
- CONTRIBUTING.md | contributor guide | explains branch flow, setup, tests, line-count rules, and the local release gate.
- LICENSE | legal | mit license grant for the workspace.
- llms.txt | metadata | concise llm/agent guidance for build, coding, and repo policy.
- pyproject.toml | python tooling config | configures ruff, target version py312, formatting, and exclusions for the local python surface.
- README.md | overview doc | project positioning, install flow, presets, cli surface, and demo corpus summary.
- rustfmt.toml | rust format policy | pins stable-channel rustfmt behavior for the workspace.
- SECURITY.md | security policy | reporting channel, scope, supported versions, and hardening notes.

## workspace directories

- crates | workspace directory | groups the four rust crates that implement format, runtime, cli, and python bindings.
- crates/nest-cli | crate directory | command-line entrypoint over nest-format and nest-runtime.
- crates/nest-cli/src | rust source dir | cli entrypoint plus subcommand modules.
- crates/nest-cli/src/cmd | rust module dir | one file per subcommand plus shared output helpers.
- crates/nest-cli/tests | test dir | integration coverage for the compiled cli binary.
- crates/nest-format | crate directory | frozen binary format, manifest, reader, writer, sections, encodings, and tests.
- crates/nest-format/examples | example dir | maintenance utilities such as golden fixture regeneration.
- crates/nest-format/src | rust source dir | format implementation organized by layout, manifest, reader, sections, writer, and codecs.
- crates/nest-format/src/encoding | rust module dir | float16, int8, int4, zstd, the intpack bitpacking primitive, and the txt_streams per-chunk-streams codec for the writer and reader.
- crates/nest-format/src/layout | rust module dir | header, footer, and section-table structs that define the on-disk container layout.
- crates/nest-format/src/manifest | rust module dir | manifest schema, canonical json serialization, and validation rules.
- crates/nest-format/src/reader | rust module dir | zero-copy parse, decode, and validation logic over byte slices.
- crates/nest-format/src/sections | rust module dir | encode and decode helpers for canonical text, spans, provenance, chunk ids, and contract payloads.
- crates/nest-format/src/writer | rust module dir | deterministic emit pipeline, payload assembly, and encoding selection.
- crates/nest-format/tests | test dir | golden, roundtrip, negative, and compatibility coverage for the format contract.
- crates/nest-format/tests/fixtures | fixture dir | byte-frozen .nest fixture used as a stable regression anchor.
- crates/nest-python | crate directory | cdylib bridge from Rust into Python through PyO3.
- crates/nest-python/src | rust source dir | module export, build glue, chunk_id helper, and NestFile wrapper.
- crates/nest-runtime | crate directory | mmap-backed runtime, ann, bm25, simd, materialization, and search.
- crates/nest-runtime/src | rust source dir | runtime entrypoints plus ann, bm25, and simd subsystems.
- crates/nest-runtime/src/ann | rust module dir | deterministic hnsw build, codec, search, and neighbor selection.
- crates/nest-runtime/src/bm25 | rust module dir | tokenizer, inverted index, codec, and reciprocal-rank fusion helpers.
- crates/nest-runtime/src/simd | rust module dir | runtime backend detection plus avx2, neon, and scalar kernels.
- crates/nest-runtime/tests | test dir | recall and exact-search integration coverage.
- dat | data dir | lfs corpus, demo datasets, and measured preset outputs.
- doc | documentation dir | usage, changelog, image assets, and architecture maps under doc/arc.
- doc/arc | documentation dir | machine-readable architecture map plus the single human architecture reference.
- python | python package dir | extension loader, builder pipeline, model fingerprint, embedder, and helper scripts.
- python/__pycache__ | cache dir | compiled python bytecode for the local interpreter.
- python/tools | python tools dir | ingestion, benchmark, and regression scripts that sit outside the public python wrapper.
- python/tools/__pycache__ | cache dir | currently empty bytecode cache directory for python/tools.
- ref | scratch dir | currently empty reference directory.
- scripts | scripts dir | shell entrypoints for repeatable repo operations.
- target | cargo artifact dir | build outputs, incremental state, copied binaries, and temporary cargo files.
- tests | python test dir | end-to-end script tests for the python and mixed cli flows.

## nest-cli

- crates/nest-cli/Cargo.toml | crate manifest | defines the nest binary and the cli-facing dependencies on nest-format, nest-runtime, clap, anyhow, and serde.
- crates/nest-cli/src/main.rs | rust source | clap entrypoint that wires eight subcommands to the corresponding cmd modules.
- crates/nest-cli/src/cmd/benchmark.rs | rust source | benchmarks exact and optional ann search, with optional madvise-cold timing and recall reporting.
- crates/nest-cli/src/cmd/cite.rs | rust source | resolves nest://content_hash/chunk_id citations back into canonical text and original spans.
- crates/nest-cli/src/cmd/inspect.rs | rust source | prints header, sections, manifest, hashes, and pretty json output through the runtime inspect path.
- crates/nest-cli/src/cmd/mod.rs | rust source | exports the cli subcommand module tree.
- crates/nest-cli/src/cmd/search.rs | rust source | parses a json vector and runs exact search through MmapNestFile.
- crates/nest-cli/src/cmd/search_ann.rs | rust source | forces the hnsw path and falls back to exact when the file has no ann section.
- crates/nest-cli/src/cmd/search_text.rs | rust source | shells out to python/embed_query.py, validates model name, dim, and model_hash, then routes by declared index_type.
- crates/nest-cli/src/cmd/stats.rs | rust source | prints file size, counts, dtype, manifest contract, per-section sizes, and simd backend.
- crates/nest-cli/src/cmd/util.rs | rust source | shared printers, encoding-name mapping, and embedder path discovery.
- crates/nest-cli/src/cmd/validate.rs | rust source | reruns full reader validation and prints a human-readable integrity report.
- crates/nest-cli/tests/cli_e2e.rs | rust test | integration coverage for the compiled cli surface and the command outputs.

## nest-format

- crates/nest-format/Cargo.toml | crate manifest | declares the frozen format crate and its serde, hash, zstd, and half dependencies.
- crates/nest-format/examples/regen_golden.rs | rust example | regenerates the byte-frozen minimal fixture and its expected hash constants.
- crates/nest-format/src/chunk.rs | rust source | defines ChunkInput, validates per-chunk invariants, and derives deterministic chunk_id values.
- crates/nest-format/src/encoding/float16.rs | rust source | float16 encode and decode helpers for compact embedding storage.
- crates/nest-format/src/encoding/int4.rs | rust source | int4 block-64 codec (encoding id 7): quantize an l2-normalized row to per-64-dim-group f16 absmax scales plus packed 4-bit signed codes; Int4EmbeddingsView parses the section (typed errors, never panics) and decodes scales/codes on demand. stored-precision, the first sub-int8 size lever.
- crates/nest-format/src/encoding/int8.rs | rust source | int8 quantization, scale handling, and parsed views over quantized embeddings.
- crates/nest-format/src/encoding/intpack.rs | rust source | shared integer bitpacking primitive (encoding id 4): per-128-block frame-of-reference, O(1) IntpackReader::get, typed errors (never panics) on truncated input. reused by the hnsw/bm25 codecs, the chunk_ids/spans repackers, and the txt_streams offset table.
- crates/nest-format/src/encoding/txt_streams.rs | rust source | per-chunk independent canonical-text streams (encoding id 10): re-layouts the chunks_canonical (0x02) compressed form into N independent zstd streams + an intpack offset table (O(1) single-chunk seek via TxtStreams). decode rebuilds the raw chunks_canonical payload BYTE-IDENTICALLY so content_hash is unchanged; typed errors (never panics) on truncated/malformed input. the named prerequisite layout for the dict/fsst text levers.
- crates/nest-format/src/encoding/mod.rs | rust source | wire-codec registry (WireCodec) that decode_payload dispatches through (incl intpack id 4 -> the content_hash-preserving chunk_ids/spans repack, and txt_streams id 10 -> the content_hash-preserving per-chunk-streams repack of chunks_canonical), the cost-driven encode_smallest try-all-pick-smallest encoder, and central encoding exports used by writer and runtime.
- crates/nest-format/src/encoding/zstd_codec.rs | rust source | zstd helpers for non-embedding sections that can be compressed without changing content_hash semantics.
- crates/nest-format/src/error.rs | rust source | typed NestError variants that cover layout, manifest, checksum, query, and input failures.
- crates/nest-format/src/layout/footer.rs | rust source | footer struct and file-hash trailer definitions.
- crates/nest-format/src/layout/header.rs | rust source | header struct, version fields, offsets, sizes, and checksum slot layout.
- crates/nest-format/src/layout/mod.rs | rust source | section ids (incl the reconciled reserved 0x11-0x16 and the per-space 0x20-0x2F / 0x30-0x3F bands), encoding ids, format constants, and layout re-exports.
- crates/nest-format/src/layout/section_entry.rs | rust source | section-table entry shape, offsets, sizes, checksums, and section-name mapping.
- crates/nest-format/src/lib.rs | rust source | public re-export surface for chunking, encoding, manifest, reader, sections, and writer APIs.
- crates/nest-format/src/manifest/canonical.rs | rust source | canonical json serialization for deterministic manifest bytes.
- crates/nest-format/src/manifest/mod.rs | rust source | manifest data model, Capabilities plus the additive CapabilitiesExt home for future capability flags, defaults, and the new-fields-are-Option-only additivity rule.
- crates/nest-format/src/manifest/validate.rs | rust source | validates dtype, metric, score_type, index_type, rerank policy, and model_hash invariants.
- crates/nest-format/src/reader/decode.rs | rust source | decodes section payloads by encoding, exposes logical bytes, and computes file_hash (whole file) and content_hash (decoded canonical six, excluding the manifest so additive manifest fields never move a citation).
- crates/nest-format/src/reader/mod.rs | rust source | zero-copy NestView surface over bytes, including section lookup and raw section access.
- crates/nest-format/src/reader/parse.rs | rust source | parses header, footer, and section table from the raw file bytes.
- crates/nest-format/src/reader/validate.rs | rust source | enforces magic, bounds, checksums, manifest, required-section validity, and the rule that embeddings cannot use zstd; non-embedding sections accept raw, zstd, intpack, or txt_streams.
- crates/nest-format/src/sections/canonical.rs | rust source | chunks_canonical raw encode/decode (length-prefixed utf-8); the byte-identical target the txt_streams repack rebuilds, and the canonical payload handling used to compute content_hash over decoded bytes.
- crates/nest-format/src/sections/chunk_ids.rs | rust source | encodes and decodes the chunk_ids section; adds the intpack repack (32 raw digest bytes) that decodes byte-identically to the raw ascii so content_hash is unchanged.
- crates/nest-format/src/sections/codec.rs | rust source | generic section payload version and count framing helpers.
- crates/nest-format/src/sections/contract.rs | rust source | search contract encoding, decoding, and manifest cross-check helpers.
- crates/nest-format/src/sections/mod.rs | rust source | section exports, shared section-level types, the intpack repack-kind constants, the decode_intpack_repack dispatch, and the decode_txt_streams dispatch (parallel) for the chunks_canonical per-chunk-streams repack.
- crates/nest-format/src/sections/provenance.rs | rust source | provenance section encode and decode helpers for free-form metadata.
- crates/nest-format/src/sections/spans.rs | rust source | original span encode and decode helpers for source_uri plus byte spans; adds the intpack repack (deduped uri pool + bitpacked offsets) that decodes byte-identically so content_hash is unchanged.
- crates/nest-format/src/writer/build.rs | rust source | low-level deterministic byte assembly and final file layout emission, keeping embeddings directly mmap-readable; under a compressed (zstd-text) preset it repacks chunk_ids/spans with intpack and takes the SMALLER of single-frame zstd vs the per-chunk txt_streams form for chunks_canonical (raw-text presets and the golden stay byte-identical).
- crates/nest-format/src/writer/encoding_choice.rs | rust source | preset-facing enums and constants for text encoding and embedding dtype.
- crates/nest-format/src/writer/mod.rs | rust source | high-level NestFileBuilder surface, manifest mutation, and optional index attachment.
- crates/nest-format/src/writer/payload.rs | rust source | prepares section payloads, prefixes, checksums, and alignment padding, with zstd limited to non-embedding payloads.
- crates/nest-format/src/writer/tests.rs | rust test | unit coverage for writer internals and deterministic output behavior.
- crates/nest-format/tests/dual_integrity.rs | rust test | checks the distinction between physical hashes and encoding-invariant content_hash.
- crates/nest-format/tests/manifest_additivity.rs | rust test | the manifest additivity guard: unset optional fields are omitted (byte-identical), the manifest round-trips byte-identically, unknown future fields survive via extra, and capabilities_ext is additive.
- crates/nest-format/tests/golden.rs | rust test | asserts the byte-frozen minimal fixture remains unchanged.
- crates/nest-format/tests/negative_fp16.rs | rust test | rejects invalid float16 payloads and value edge cases.
- crates/nest-format/tests/int4_roundtrip.rs | rust test | int4 codec round-trip and unit coverage (pack/unpack, quantize clamping, the section view, decode_payload borrow, and typed malformed-payload rejection); lives out of src so the codec stays under the 300-line guard.
- crates/nest-format/tests/int8_roundtrip.rs | rust test | int8 codec positive round-trip coverage relocated out of encoding/mod.rs to keep the wire-codec registry under the 300-line guard.
- crates/nest-format/tests/negative_int4.rs | rust test | rejects malformed int4 files: bad payload version, bad scale_kind, NaN/Inf group scales, truncation, and dim-not-multiple-of-64 at the section level.
- crates/nest-format/tests/negative_int8.rs | rust test | rejects malformed int8 payloads, scales, and truncation.
- crates/nest-format/tests/negative_zstd.rs | rust test | rejects invalid zstd use and illegal encoding combinations.
- crates/nest-format/tests/txt_streams_roundtrip.rs | rust test | txt_streams codec positive coverage: encode -> decode_payload(id 10) and the sections-level decode_txt_streams rebuild bytes BYTE-IDENTICAL to encode_chunks_canonical (the content_hash invariant) over empty/single/many/multibyte-utf8 corpora, plus O(1) offset-table seek and deterministic re-encode.
- crates/nest-format/tests/negative_txt_streams.rs | rust test | rejects malformed txt_streams payloads (empty, bad kind byte, truncated count/offset-table/stream, oversized claimed count, corrupted zstd) with a typed NestError, NEVER a panic; exhaustive prefix-truncation fuzz.
- crates/nest-format/tests/reserved_ids.rs | rust test | asserts the reconciled additive section-id bands (0x09-0x16, 0x20-0x2F, 0x30-0x3F) are disjoint, excluded from content_hash, and unresolved by section_name until each feature ships.
- crates/nest-format/tests/roundtrip.rs | rust test | exercises roundtrip behavior across raw, zstd, float16, int8, and int4 combinations, incl the int4 preset's canonical-decode stability, deterministic build, and content_hash difference vs float32.
- crates/nest-format/tests/mrl_truncate.rs | rust test | matryoshka truncate-then-renormalize roundtrip: the stored embeddings section equals the manual prefix-slice + l2-renorm byte-for-byte, two builds at the same mrl_dim are byte-identical (file_hash equal), and the truncated content_hash differs from full-dim.
- crates/nest-format/tests/v01_compat.rs | rust test | ensures v0.1 fixtures still load under the v0.2 reader.

## nest-python

- crates/nest-python/Cargo.toml | crate manifest | defines the cdylib target and the pyo3-facing dependency surface.
- crates/nest-python/build.rs | rust build script | emits PyO3 cfgs and macOS dynamic_lookup linker flags for extension loading.
- crates/nest-python/src/build_fn.rs | rust source | exposes build() to python, resolves presets, optionally builds hnsw and bm25 payloads, and applies build-time matryoshka prefix truncation (the mrl_dim kwarg: slice each row to the prefix, re-l2-normalize, set embedding_dim=mrl_dim + record full_dim) before calling NestFileBuilder, so quant/hnsw see the shorter renormalized row. input parsing and the truncate+renorm helper live in build_inputs.rs to keep this entry point under the 300-line guard.
- crates/nest-python/src/build_inputs.rs | rust source | build() input helpers: parse_chunks (PyList of dicts -> Vec<ChunkInput>, typed errors) and truncate_renormalize (matryoshka prefix slice + l2-renorm in place, applied before quantization/hnsw).
- crates/nest-python/src/chunk_id_fn.rs | rust source | exposes the Rust chunk_id derivation to python callers.
- crates/nest-python/src/lib.rs | rust source | registers NestFile, SearchHitPy, build(), and chunk_id() into the _nest module.
- crates/nest-python/src/nest_file.rs | rust source | wraps MmapNestFile for python and re-exposes search, inspect, validate, and metadata getters.

## nest-runtime

- crates/nest-runtime/Cargo.toml | crate manifest | declares the runtime crate and its dependencies on nest-format, mmap, rayon, half, serde, and sha2.
- crates/nest-runtime/src/ann/build.rs | rust source | deterministic hnsw graph construction over normalized vectors.
- crates/nest-runtime/src/ann/codec.rs | rust source | serializes and deserializes the optional hnsw section payload; v2 bitpacks the level/count/neighbour-id columns with intpack (order-preserving, recall unchanged) and still reads v1.
- crates/nest-runtime/src/ann/mod.rs | rust source | hnsw index types, defaults, distance model, and roundtrip tests.
- crates/nest-runtime/src/ann/search.rs | rust source | performs ann candidate exploration over the hnsw graph.
- crates/nest-runtime/src/ann/select_neighbors.rs | rust source | neighbor selection heuristic used during graph construction.
- crates/nest-runtime/src/bm25/codec.rs | rust source | serializes and deserializes the optional bm25 section payload; v2 delta-gaps the sorted doc ids and bitpacks gaps/tfs/doc-lengths with intpack (scores unchanged) and still reads v1.
- crates/nest-runtime/src/bm25/fusion.rs | rust source | reciprocal-rank fusion helper that merges lexical and vector candidates.
- crates/nest-runtime/src/bm25/index.rs | rust source | builds and queries the bm25 inverted index kept in memory at open time.
- crates/nest-runtime/src/bm25/mod.rs | rust source | bm25 module surface, payload contract, defaults, and tokenizer notes.
- crates/nest-runtime/src/bm25/tests.rs | rust test | unit coverage for tokenization, indexing, and lexical scoring behavior.
- crates/nest-runtime/src/bm25/tokenize.rs | rust source | lowercases and tokenizes on unicode-aware alphanumeric boundaries.
- crates/nest-runtime/src/error.rs | rust source | RuntimeError surface that wraps format errors and query validation failures.
- crates/nest-runtime/src/lib.rs | rust source | public SearchHit and SearchResult types plus runtime re-exports.
- crates/nest-runtime/src/materialize.rs | rust source | PackedVectors store for the ann graph: keeps int8/int4/float16 rows in their on-disk packing and decodes one row at a time into a scratch buffer, removing the old n*dim*4 f32 snapshot while keeping distances byte-identical.
- crates/nest-runtime/src/rerank.rs | rust source | the explicit rerank-source handle (RerankSource) the exact-cosine recompute reads through (f32/f16/int8/int4 slab via the simd kernels), plus FpSlab detection for the optional embeddings_fp (0x09) full-precision slab; ready for per-space routing.
- crates/nest-runtime/src/mmap_file.rs | rust source | owns the mmap, decodes metadata and optional indices at open time (including the optional embeddings_fp slab and the packed ann vector store), and exposes inspect and revalidate helpers while assuming embeddings stay directly readable from the mapped file.
- crates/nest-runtime/src/search.rs | rust source | validates queries, scores exact rows and reranks ann and hybrid candidates through the single rerank source, and materializes stable hit contracts.
- crates/nest-runtime/src/simd/avx2.rs | rust source | x86_64 avx2 kernels for dot products over f32, int8, and the fused int4 block-64 dequant+dot hot paths.
- crates/nest-runtime/src/simd/mod.rs | rust source | once-only backend detection plus dispatch for scalar, avx2, and neon kernels (incl dot_f32_i4_blocked).
- crates/nest-runtime/src/simd/neon.rs | rust source | aarch64 neon kernels, including float16, int8, and the fused int4 block-64 dequant+dot paths.
- crates/nest-runtime/src/simd/scalar.rs | rust source | portable scalar fallback for all dtype combinations, incl the int4 per-group reference reduction the simd backends match bit-for-bit.
- crates/nest-runtime/src/simd/tests.rs | rust test | parity coverage across scalar and simd implementations, incl the int4 bit-for-bit backend-parity and reference-dequant-dot checks.
- crates/nest-runtime/tests/fp16_topk_recall_vs_f32.rs | rust test | recall and score-drift checks for float16 against float32 exact search.
- crates/nest-runtime/tests/hnsw_recall.rs | rust test | release-mode recall floors for the ann path.
- crates/nest-runtime/tests/rerank_contract.rs | rust test | the honest-rerank gate: every non-exact path returns the exact-cosine rerank score byte-for-byte and recall is NaN; parameterized over all search entry points.
- crates/nest-runtime/tests/search_exact.rs | rust test | exact search behavior, query validation, and stable ordering tests.

## data and docs

- doc/arc/arc.md | markdown doc | single human architecture inventory and contract reference for the repo.
- doc/arc/arc.yaml | yaml doc | machine-readable architecture map for agents, llms, and maintenance workflows.
- doc/arc/arc.mmd | mermaid doc | sequence diagram of the build and query flows.
- dat | data dir | top-level home for the demo sources, baseline corpus, and measure outputs.
- doc/changelog.md | markdown doc | release deltas, compatibility notes, and test-surface growth between v0.1 and v0.2.
- doc/usage.md | markdown doc | operator-facing usage guide for build, validate, stats, inspect, search, benchmark, and citations.
- dat/demo/README.md | markdown doc | explains the seven PT-BR datasets, dedupe process, and reproducible corpus build inputs.

## python tooling

- python/_nest.so | binary artifact | compiled PyO3 extension loaded by python/nest.py.
- python/builder.py | python source | reusable ingestion pipeline that chunks text, caches embeddings in sqlite, emits .nest files, and validates output.
- python/convert_legacy.py | python source | migrates the legacy sqlite truw corpus into the new deterministic v1 binary format.
- python/embed_query.py | python source | search-text helper that embeds one query, normalizes it, fingerprints the local model snapshot, and prints structured json.
- python/forge/__init__.py | python source | forge python package surface; re-exports the SEMANTIC default embedder (potion) plus the lexical floor (lexical_embedder).
- python/forge/embed_potion.py | python source | the DEFAULT offline SEMANTIC static embedder: vendored model2vec/potion-base-8M table, numpy + tokenizers only (no torch, no network), tokenize -> gather token rows -> mean pool -> l2 normalize; offline by construction; self-fingerprints to a sha256 model_hash over the vendored files. drop-in with the floor's interface; reproduces the model2vec reference vector-for-vector.
- python/forge/models/potion-base-8M/ | data | vendored offline embedder table (mit): model.safetensors [29528,256] f32 (~30mb, git-lfs) + tokenizer.json + model2vec config; the semantic default's asset.
- python/forge/embed_default.py | python source | the lexical FLOOR / zero-dep fallback (stdlib-only): tokenize, fixed pseudo-random token vectors, l2-normalized mean, f32-stable; self-fingerprints to a sha256 model_hash. captures literal token overlap only, no semantic generalization.
- python/forge/demo_corpus/ | data | license-clean (cc0, original) demo doc folder for the flagship one-gif demo; builds a byte-identical .nest offline with the static embedder.
- python/forge/test_embed_default.py | python test | floor self-test (stdlib): determinism, f32-stability, normalization, lexical signal, fingerprint stability, demo corpus presence.
- python/forge/test_embed_potion.py | python test | potion self-test (numpy+tokenizers): semantic jump (car~automobile >> car~banana), determinism, f32-stability, no-network leak, interface parity, model_hash stability.
- python/forge/recall_harness.py | python tool | side-by-side per-query recall of potion vs the lexical floor on demo_corpus, proving semantic retrieval beats keyword overlap.
- python/model_fingerprint.py | python source | computes a reproducible model fingerprint and compact model_hash from inference-relevant files only.
- python/nest.py | python source | dynamic loader and stable public python API over the _nest extension.
- python/tools/_baseline_decoder.py | python source | decodes a baseline .nest directly so preset measurement can rebuild variants without re-embedding.
- python/tools/_bench_runner.py | python source | pure timing and variant-build helpers for preset measurement.
- python/tools/_corpus_sources.py | python source | dataset loader registry that normalizes the seven PT-BR sources into a common dataframe shape.
- python/tools/compare_measure.py | python source | validates measured preset metrics against regression gates and baseline headroom.
- python/tools/measure_presets.py | python source | builds exact, compressed, tiny, nano, and hybrid variants and measures size, recall, drift, and latency.
- python/tools/nest_build_corpus.py | python source | end-to-end corpus builder over the PT-BR demo sources, embedding model, cache, and validation flow.

## scripts, tests, and artifacts

- scripts/release_check.sh | shell script | authoritative release pipeline that builds, tests, lints, rebuilds _nest.so, measures presets, and compares against baseline.json.
- tests/test_builder.py | python test | covers chunk byte spans, pipeline emit, deterministic cache reuse, and reproducible output behavior.
- tests/test_e2e.py | python test | covers the in-process PyO3 path for build, search, inspect, validate, and search-hit contract fields.
- tests/test_search_text_model_hash.py | python test | covers search-text match, mismatch, placeholder, skip, and dim mismatch cases without requiring a real model.
- python/__pycache__ | cache dir | contains generated .pyc files for builder.py and nest.py.
- python/tools/__pycache__ | cache dir | empty generated cache directory for the python/tools package.
- ref | empty dir | no current implementation content.
- target | cargo artifact dir | generated build tree, including debug, release, and target/tmp.

## forge-core (separate workspace, ingestion layer)

forge-core is a sibling cargo workspace at the repo root, deliberately OUTSIDE crates/, so its dependency tree never enters nest-format or nest-runtime. it is FORGE-0a: the frozen .fci schema only. build and test it with its own manifest (`cargo build --manifest-path forge-core/Cargo.toml`); the sovereign `cargo build --workspace` does not see it.

- forge-core/Cargo.toml | crate manifest | standalone workspace plus package; minimal deps (serde, serde_json, thiserror); edition 2024.
- forge-core/src/lib.rs | rust source | crate surface and the boundary doc: no chunker, no model runtime; determinism anchored on canonical text plus per-space fingerprint.
- forge-core/src/error.rs | rust source | typed ForgeError variants (serialize, deserialize, invalid, unsupported schema version); never panics.
- forge-core/src/fci/mod.rs | rust source | FciBundle container, FCI_SCHEMA_VERSION, and validate (cross-reference plus schema-version checks).
- forge-core/src/fci/record.rs | rust source | ChunkRecord, mirroring builder.ChunkSpec exactly so the adapter is a 1:1 map and spans round-trip through nest cite.
- forge-core/src/fci/embedding_request.rs | rust source | EmbeddingRequest, SpaceTag, and PayloadRef, the multimodal carrier (one chunk, several named-space requests).
- forge-core/src/fci/entity.rs | rust source | Entity, MentionSpan, and the typed weighted Edge for the graph pillar.
- forge-core/src/fci/blob_ref.rs | rust source | BlobRef, a content-hash reference to an original artifact for catalog mode.
- forge-core/src/fci/serialize.rs | rust source | deterministic canonical serialize and deserialize (compact json, declaration order, verbatim strings) plus roundtrip, determinism, and validate tests.

## coverage

- every user-requested path is covered in this inventory.