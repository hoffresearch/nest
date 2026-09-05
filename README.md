![nest](/doc/nest-hoff-research-db.png)

# nest

single-file, memory-mapped, hash-verified vector database with stable citations. one `.nest` file carries chunks, embeddings, source spans, media, indices, and a search contract; a rust runtime mmaps it and answers with exact-cosine scores and `nest://content_hash/chunk_id` references that survive re-encoding. reproducible byte for byte, offline by construction. sovereign in the plain sense: the file is the whole database and nothing phones home.

python builds. rust serves. nest ships. agents/llms read, that's it.

no server, no api call, no central index. ship a curated knowledge base with the application. the file is the database.

## sovereign, enforced by the format

- **self-contained**: the file is the entire knowledge base; copy it like a sqlite db.
- **verifiable**: sha-256 per section, per file, and over the decoded content; every hit returns a `nest://content_hash/chunk_id` citation that `nest cite` resolves to the stored canonical text.
- **reproducible**: same chunks + same model fingerprint + `reproducible=True` = byte-identical `file_hash` on any machine.
- **offline-first**: the runtime never opens a socket; model mismatches fail loudly via the `model_hash` gate, never silently.

## architecture

python builds a deterministic container; a rust runtime mmaps it and answers exact, hnsw, bm25, graph, and per-space searches, always finishing with an exact-cosine rerank. the cli and the python api are thin surfaces over the same runtime.

- `nest-format`: frozen v1 container (layout, manifest, sections, encodings, hashes)
- `nest-runtime`: mmap, simd dispatch, indices, search with mandatory exact rerank
- `nest-cli`: the `nest` binary (engine verbs + `ask`/`retrieve` + declarative `build`)
- `nest-python`: pyo3 bridge (`nest.open`, `nest.build`, `NestFile.retrieve`)
- `python/`: writer pipeline, model registry, offline embedders, forge tooling

the full map (flows, contracts, inventory) lives in [doc/arc/arc.yaml](doc/arc/arc.yaml) and the visual sequence in [doc/arc/arc.mmd](doc/arc/arc.mmd).

## install

```
curl -sSf https://raw.githubusercontent.com/hoffresearch/nest/main/scripts/install.sh | sh
nest doctor
pip install "nestdb[embed]"     # python; offline embedding via the bundled potion table
```

also windows (`install.ps1`), homebrew tap, `cargo binstall nest-cli`, docker. artifacts carry sha256 + sigstore attestations. channels, verification, and offline notes: [doc/install.md](doc/install.md).

dev build (rust edition 2024, python 3.12+):

```
cargo build --release --workspace
cargo build --release -p nest-python --features pyo3/extension-module
cp target/release/lib_nest.dylib python/_nest.so   # macOS (.so on linux)
```

## cli

one binary, two groups of verbs. the engine takes a file and a vector and never runs python:

```
nest search       <file> <qvec> -k K            exact top-k
nest search-ann   <file> <qvec> -k K --ef N     hnsw + exact rerank
nest search-graph <file> <qvec> -k K --hops N   chunk-graph bfs + exact rerank
nest search-space <file> <qvec> --space NAME    one named multimodal space
nest search-text  <file> "query" -k K           embed + model_hash gate + route
nest media        <file> [--export DIR]         list / export the inlined media blobs, sha256-verified
nest inspect | validate | stats | cite | benchmark | doctor
```

the agent verbs take text or a build spec, shell out to the offline python embedder or the forge, and speak in cited answers:

```
nest ask          <file> "query" -k K           cited answer, offline
nest retrieve     <file> "query" -k K           json/jsonl cited answer-pack, score = exact rerank
nest build        --spec corpus.toml            declarative corpus build (source + media + models)
```

`build` takes one toml describing the source (sqlite query, csv/jsonl, image dir), the media (av1/avif/jxl, dedup, `crf="auto"` dual quality gate), and one or several embedding models from the registry (`potion`, `clip-vit-b32`, `siglip2`, `wemm-2b`, ...), each a named vector space in the same file. `ask`/`retrieve` embed offline, validate `model_hash` against the manifest, and every printed score is the exact-cosine rerank value. contract and knobs, with a full worked spec: [doc/usage.md](doc/usage.md) §13.

## python

```python
import sys; sys.path.insert(0, "python"); import nest

db = nest.open(path)
hits = db.retrieve(qvec, k=5)      # cited: text, citation_id, score = exact rerank
db.search(qvec, k=5); db.search_ann(qvec, k=5, ef=100)
db.search_hybrid(qvec, query_text, k=5); db.search_space("clip-vit-b32", ivec, 5)
db.validate()

nest.build(output_path, embedding_model, embedding_dim, chunker_version,
           model_hash, chunks, reproducible=True, preset="hybrid")
```

or `Pipeline` in `python/builder.py` (chunker, sqlite cache, auto-validate). offline demo: `python python/forge/retrieve.py`.

## benchmarks

[doc/benchmarks.md](doc/benchmarks.md): nest against usearch, hnswlib, sqlite-vec and lancedb on the same rows, same machine, same ruler, including the columns where nest is ordinary (hnsw build time) and the rows for what it does not do (updates, filters, concurrent writers). reproduce with `python/tools/bench_competitors.py`.

## hardening

- `clippy::unwrap_used` and `clippy::undocumented_unsafe_blocks` are denied workspace-wide; `nest-format` has zero `unsafe`, the runtime's `unsafe` is the SIMD kernels and two `mmap` calls, each with its invariant written down.
- every push runs [ci.yml](.github/workflows/ci.yml): fmt, clippy, tests on linux (avx2) and macos (neon), a deterministic mutation-fuzz harness over every section decoder and search verb, and a coverage-guided `cargo fuzz` smoke; `fuzz/` holds the targets and seeds.
- a malformed `.nest` that panics the runtime is a security bug: [doc/SECURITY.md](doc/SECURITY.md). what is still open, with a spec per item: [doc/hardening-plan.md](doc/hardening-plan.md).

## presets

| preset       | text | embeddings  | ann | bm25 | size ratio | recall@10 |
|--------------|------|-------------|-----|------|-----------:|----------:|
| `exact`      | raw  | float32     | no  | no   |     1.000  |   1.0000  |
| `compressed` | zstd | float16     | no  | no   |     0.339  |   1.0000  |
| `tiny`       | zstd | int8        | yes | no   |     0.256  |   0.9920  |
| `micro`      | zstd | mrl256-int8 | yes | no   |     0.223  |   0.8100  |
| `nano`       | zstd | int4        | yes | no   |     0.209  |   0.9130  |
| `hybrid`     | zstd | float32     | yes | yes  |     0.609  |   1.0000  |

measured on a 30,725-chunk pt-br corpus (`dat/measure/ladder.json`, gated in ci). the recall ruler is self-perturbation, so it reports rank stability under quantization, not real-query quality; sub-int8 scores are real cosine at the stored precision, disclosed on every result. full honesty notes, the mrl curve, and the lever guide: [doc/usage.md](doc/usage.md) section 6.

## reference

- [doc/usage.md](doc/usage.md): every verb, presets, offline mode, model registry, declarative builds, compression levers
- [doc/arc/arc.yaml](doc/arc/arc.yaml) + [doc/arc/arc.mmd](doc/arc/arc.mmd): the architecture pair
- [doc/benchmarks.md](doc/benchmarks.md): the competitor table and how it was measured
- [doc/hardening-plan.md](doc/hardening-plan.md): what a reviewer found, what was fixed, what is open
- [doc/changelog.md](doc/changelog.md): releases and unreleased deltas, with measured numbers
- [doc/install.md](doc/install.md): every install channel and its verification
- [dat/demo/Instructions.md](dat/demo/Instructions.md): the pt-br demo corpus sources and rebuild
- [.contracts/.agents/AGENTS.md](.contracts/.agents/AGENTS.md): the single instruction source for agents and contributors
- `./scripts/release_check.sh`: the merge gate; it documents itself by being the gate

## license

made it simple, but significant
Hoff Research   hoffresearch.com
Brenner Cruvinel
(∂μfμν = jν)
MIT
