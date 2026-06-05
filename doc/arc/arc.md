---
project: nest
last-updated: "2026-06-05"
domain: architecture
---

# architecture inventory

this file is the human readable companion to doc/arc/arc.yaml and doc/arc/arc.mmd. it is the single human architecture reference in this directory and records what each requested path does and how it fits into the repo.

the trio is kept in sync: arc.md (human), arc.yaml (machine), arc.mmd (visual). any change to boundaries, data flow, sections, or runtime contract must update all three in the same commit.

## system view

- nest-format is the frozen v1 container contract. it owns layout, manifest, reader, writer, canonical sections, hashes, and wire encodings. embeddings stay raw float32, float16, or int8; zstd is reserved for non-embedding sections.
- nest-runtime opens .nest files through mmap, validates queries, dispatches SIMD, and executes exact, ann, and hybrid search while reranking candidates with real cosine. that hot path is why embeddings are never zstd-compressed.
- nest-cli is a thin clap surface over the runtime and format crates. each command does argument handling and output formatting, not core search math.
- nest-python is the PyO3 bridge. python/nest.py loads the compiled extension, python/builder.py orchestrates chunking and caching, and python/embed_query.py preserves model parity for search-text.
- scripts/release_check.sh is the release gate. it rebuilds the extension, runs rust and python suites, checks lint and format, measures presets, and compares against the committed baseline.

## format and runtime contract

- required sections are chunk_ids, chunks_canonical, chunks_original_spans, embeddings, provenance, and search_contract. optional sections are hnsw_index and bm25_index.
- additive section ids past the implemented 0x01-0x08 are reserved in one reconciled, disjoint map in layout/mod.rs: 0x09-0x10 (scalar reservations: embeddings_fp, dictionary, dedup_map, graph_adjacency, chunk_scalars, tokenizer_model, edit_journal, repro_manifest), 0x11-0x16 (graph_nodes, graph_edge_props, graph_entity_map, blob_refs, space_table, blob_span_overlay), and the per-space bands 0x20-0x2F (space embeddings) and 0x30-0x3F (space fp). all are excluded from content_hash and unresolved by section_name until their feature ships; tests/reserved_ids.rs asserts the bands are disjoint and content_hash-safe. wire encodings reserve 4-9 (intpack, zstd_dict, frontcode, int4, rabitq, fsst); decode_payload dispatches through a wire-codec registry that rejects reserved-but-unimplemented ids.
- raw and zstd apply to non-embedding payloads. float16 and int8 apply only to embeddings. embeddings never use zstd because the runtime scores them directly from mmap through the simd path.
- the four integrity surfaces are header_checksum, section checksum over physical bytes, file_hash over the whole pre-footer body, and content_hash over decoded canonical sections.
- exact search is always the ground truth path. hnsw and hybrid search return final hits only after exact cosine rerank.
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
- crates/nest-format/src/encoding | rust module dir | float16, int8, and zstd codec support for the writer and reader.
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
- crates/nest-format/src/encoding/int8.rs | rust source | int8 quantization, scale handling, and parsed views over quantized embeddings.
- crates/nest-format/src/encoding/mod.rs | rust source | wire-codec registry (WireCodec) that decode_payload dispatches through, the cost-driven encode_smallest try-all-pick-smallest encoder, and central encoding exports used by writer and runtime.
- crates/nest-format/src/encoding/zstd_codec.rs | rust source | zstd helpers for non-embedding sections that can be compressed without changing content_hash semantics.
- crates/nest-format/src/error.rs | rust source | typed NestError variants that cover layout, manifest, checksum, query, and input failures.
- crates/nest-format/src/layout/footer.rs | rust source | footer struct and file-hash trailer definitions.
- crates/nest-format/src/layout/header.rs | rust source | header struct, version fields, offsets, sizes, and checksum slot layout.
- crates/nest-format/src/layout/mod.rs | rust source | section ids (incl the reconciled reserved 0x11-0x16 and the per-space 0x20-0x2F / 0x30-0x3F bands), encoding ids, format constants, and layout re-exports.
- crates/nest-format/src/layout/section_entry.rs | rust source | section-table entry shape, offsets, sizes, checksums, and section-name mapping.
- crates/nest-format/src/lib.rs | rust source | public re-export surface for chunking, encoding, manifest, reader, sections, and writer APIs.
- crates/nest-format/src/manifest/canonical.rs | rust source | canonical json serialization for deterministic manifest bytes.
- crates/nest-format/src/manifest/mod.rs | rust source | manifest data model, capability flags, defaults, and test helpers.
- crates/nest-format/src/manifest/validate.rs | rust source | validates dtype, metric, score_type, index_type, rerank policy, and model_hash invariants.
- crates/nest-format/src/reader/decode.rs | rust source | decodes section payloads by encoding and exposes logical bytes to higher layers.
- crates/nest-format/src/reader/mod.rs | rust source | zero-copy NestView surface over bytes, including section lookup and raw section access.
- crates/nest-format/src/reader/parse.rs | rust source | parses header, footer, and section table from the raw file bytes.
- crates/nest-format/src/reader/validate.rs | rust source | enforces magic, bounds, checksums, manifest, required-section validity, and the rule that embeddings cannot use zstd.
- crates/nest-format/src/sections/canonical.rs | rust source | canonical payload handling used to compute content_hash over decoded bytes.
- crates/nest-format/src/sections/chunk_ids.rs | rust source | encodes and decodes the chunk_ids section.
- crates/nest-format/src/sections/codec.rs | rust source | generic section payload version and count framing helpers.
- crates/nest-format/src/sections/contract.rs | rust source | search contract encoding, decoding, and manifest cross-check helpers.
- crates/nest-format/src/sections/mod.rs | rust source | section exports and shared section-level types.
- crates/nest-format/src/sections/provenance.rs | rust source | provenance section encode and decode helpers for free-form metadata.
- crates/nest-format/src/sections/spans.rs | rust source | original span encode and decode helpers for source_uri plus byte spans.
- crates/nest-format/src/writer/build.rs | rust source | low-level deterministic byte assembly and final file layout emission, keeping embeddings directly mmap-readable.
- crates/nest-format/src/writer/encoding_choice.rs | rust source | preset-facing enums and constants for text encoding and embedding dtype.
- crates/nest-format/src/writer/mod.rs | rust source | high-level NestFileBuilder surface, manifest mutation, and optional index attachment.
- crates/nest-format/src/writer/payload.rs | rust source | prepares section payloads, prefixes, checksums, and alignment padding, with zstd limited to non-embedding payloads.
- crates/nest-format/src/writer/tests.rs | rust test | unit coverage for writer internals and deterministic output behavior.
- crates/nest-format/tests/dual_integrity.rs | rust test | checks the distinction between physical hashes and encoding-invariant content_hash.
- crates/nest-format/tests/golden.rs | rust test | asserts the byte-frozen minimal fixture remains unchanged.
- crates/nest-format/tests/negative_fp16.rs | rust test | rejects invalid float16 payloads and value edge cases.
- crates/nest-format/tests/negative_int8.rs | rust test | rejects malformed int8 payloads, scales, and truncation.
- crates/nest-format/tests/negative_zstd.rs | rust test | rejects invalid zstd use and illegal encoding combinations.
- crates/nest-format/tests/reserved_ids.rs | rust test | asserts the reconciled additive section-id bands (0x09-0x16, 0x20-0x2F, 0x30-0x3F) are disjoint, excluded from content_hash, and unresolved by section_name until each feature ships.
- crates/nest-format/tests/roundtrip.rs | rust test | exercises roundtrip behavior across raw, zstd, float16, and int8 combinations.
- crates/nest-format/tests/v01_compat.rs | rust test | ensures v0.1 fixtures still load under the v0.2 reader.

## nest-python

- crates/nest-python/Cargo.toml | crate manifest | defines the cdylib target and the pyo3-facing dependency surface.
- crates/nest-python/build.rs | rust build script | emits PyO3 cfgs and macOS dynamic_lookup linker flags for extension loading.
- crates/nest-python/src/build_fn.rs | rust source | exposes build() to python, resolves presets, and optionally builds hnsw and bm25 payloads before calling NestFileBuilder.
- crates/nest-python/src/chunk_id_fn.rs | rust source | exposes the Rust chunk_id derivation to python callers.
- crates/nest-python/src/lib.rs | rust source | registers NestFile, SearchHitPy, build(), and chunk_id() into the _nest module.
- crates/nest-python/src/nest_file.rs | rust source | wraps MmapNestFile for python and re-exposes search, inspect, validate, and metadata getters.

## nest-runtime

- crates/nest-runtime/Cargo.toml | crate manifest | declares the runtime crate and its dependencies on nest-format, mmap, rayon, half, serde, and sha2.
- crates/nest-runtime/src/ann/build.rs | rust source | deterministic hnsw graph construction over normalized vectors.
- crates/nest-runtime/src/ann/codec.rs | rust source | serializes and deserializes the optional hnsw section payload.
- crates/nest-runtime/src/ann/mod.rs | rust source | hnsw index types, defaults, distance model, and roundtrip tests.
- crates/nest-runtime/src/ann/search.rs | rust source | performs ann candidate exploration over the hnsw graph.
- crates/nest-runtime/src/ann/select_neighbors.rs | rust source | neighbor selection heuristic used during graph construction.
- crates/nest-runtime/src/bm25/codec.rs | rust source | serializes and deserializes the optional bm25 section payload.
- crates/nest-runtime/src/bm25/fusion.rs | rust source | reciprocal-rank fusion helper that merges lexical and vector candidates.
- crates/nest-runtime/src/bm25/index.rs | rust source | builds and queries the bm25 inverted index kept in memory at open time.
- crates/nest-runtime/src/bm25/mod.rs | rust source | bm25 module surface, payload contract, defaults, and tokenizer notes.
- crates/nest-runtime/src/bm25/tests.rs | rust test | unit coverage for tokenization, indexing, and lexical scoring behavior.
- crates/nest-runtime/src/bm25/tokenize.rs | rust source | lowercases and tokenizes on unicode-aware alphanumeric boundaries.
- crates/nest-runtime/src/error.rs | rust source | RuntimeError surface that wraps format errors and query validation failures.
- crates/nest-runtime/src/lib.rs | rust source | public SearchHit and SearchResult types plus runtime re-exports.
- crates/nest-runtime/src/materialize.rs | rust source | materializes stored embeddings into f32 vectors for ann build and attach paths.
- crates/nest-runtime/src/mmap_file.rs | rust source | owns the mmap, decodes metadata and optional indices at open time, and exposes inspect and revalidate helpers while assuming embeddings stay directly readable from the mapped file.
- crates/nest-runtime/src/search.rs | rust source | validates queries, scores exact rows, reranks ann and hybrid candidates, and materializes stable hit contracts.
- crates/nest-runtime/src/simd/avx2.rs | rust source | x86_64 avx2 kernels for dot products over f32 and int8 hot paths.
- crates/nest-runtime/src/simd/mod.rs | rust source | once-only backend detection plus dispatch for scalar, avx2, and neon kernels.
- crates/nest-runtime/src/simd/neon.rs | rust source | aarch64 neon kernels, including float16 and int8 specialized paths.
- crates/nest-runtime/src/simd/scalar.rs | rust source | portable scalar fallback for all dtype combinations.
- crates/nest-runtime/src/simd/tests.rs | rust test | parity coverage across scalar and simd implementations.
- crates/nest-runtime/tests/fp16_topk_recall_vs_f32.rs | rust test | recall and score-drift checks for float16 against float32 exact search.
- crates/nest-runtime/tests/hnsw_recall.rs | rust test | release-mode recall floors for the ann path.
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
- python/model_fingerprint.py | python source | computes a reproducible model fingerprint and compact model_hash from inference-relevant files only.
- python/nest.py | python source | dynamic loader and stable public python API over the _nest extension.
- python/tools/_baseline_decoder.py | python source | decodes a baseline .nest directly so preset measurement can rebuild variants without re-embedding.
- python/tools/_bench_runner.py | python source | pure timing and variant-build helpers for preset measurement.
- python/tools/_corpus_sources.py | python source | dataset loader registry that normalizes the seven PT-BR sources into a common dataframe shape.
- python/tools/compare_measure.py | python source | validates measured preset metrics against regression gates and baseline headroom.
- python/tools/measure_presets.py | python source | builds exact, compressed, tiny, and hybrid variants and measures size, recall, drift, and latency.
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

## coverage

- every user-requested path is covered in this inventory.