![urna: offline-first vector database, rust and python](https://raw.githubusercontent.com/hoffresearch/urna/main/assets/images/urna-hoff-research-db-iage-thumb-git.png)

# Urna

A vector database in one file, with citations that stay valid.

A `.urna` file holds the chunks, the embeddings, the source spans, the indices and the search contract. The rust runtime maps it into memory, checks its hashes, and answers with exact cosine scores and a `urna://content_hash/chunk_id` citation for every hit. It works offline and rebuilds byte for byte. Python builds the file, rust serves it.

## Install

```sh
brew tap hoffresearch/urna
brew install urna
```

```sh
npm install -g @urna/cli
```

bun, pnpm and yarn install the same package: `bun add -g @urna/cli`, `pnpm add -g @urna/cli`, `yarn global add @urna/cli`.

```sh
cargo install urna
```

```sh
curl -sSf https://raw.githubusercontent.com/hoffresearch/urna/main/scripts/install.sh | sh
```

Then run setup once. It downloads the offline embedder, prepares a python env and checks the install:

```sh
urna setup
```

Python only, no setup step needed:

```sh
pip install "urna[embed]"
```

Windows, docker, `cargo binstall` and how to verify a download are in the [install reference](https://github.com/hoffresearch/urna/blob/main/docs/USAGE.md#reference).

## In the terminal

`urna setup` shows the plan before it writes anything and ends on the doctor checks.

<img src="https://raw.githubusercontent.com/hoffresearch/urna/main/assets/images/urna-setup.png" alt="urna setup: the verify step with every doctor check passing" width="100%">

`urna tui` opens a corpus, validates it, and lets you ask it questions. Each hit shows its score, the stored text and its citation.

```sh
urna tui my_corpus.urna
```

<img src="https://raw.githubusercontent.com/hoffresearch/urna/main/assets/images/urna-tui.png" alt="urna tui: the ask tab with scored hits and the cited text of the selected one" width="100%">

## Quickstart

`examples/quickstart/` has twelve short paragraphs and the spec that builds them. From a checkout:

```sh
urna build --spec examples/quickstart/corpus.toml
```

```sh
urna ask examples/quickstart/out/quickstart.urna "can I use this offline" -k 1
```

```sh
urna retrieve examples/quickstart/out/quickstart.urna "how do citations work" -k 2 --format jsonl
```

```sh
urna cite examples/quickstart/out/quickstart.urna 'urna://sha256:1147b256.../sha256:b5dfeb09...'
```

```sh
urna validate examples/quickstart/out/quickstart.urna
```

`ask` prints the answer with its citation, `retrieve` prints json for another program, `cite` turns a citation back into the stored text, and `validate` checks every hash. To build from your own rows, see [usage section 13](https://github.com/hoffresearch/urna/blob/main/docs/USAGE.md).

## What the file guarantees

| Property | How |
|----------|-----|
| Self-contained | The file is the whole database. Copy it like a sqlite file. |
| Verifiable | Sha-256 over the whole file and over the decoded content, plus a checksum on the header and on every section. `urna cite` resolves any citation. |
| Reproducible | Same chunks and same model give a byte-identical file on any machine. |
| Offline | The runtime never opens a socket. A query from the wrong model fails at the `model_hash` check. |

## Python

```python
import urna
from urna.embed_potion import potion_embedder

emb = potion_embedder()
db = urna.open("my_corpus.urna")
qvec = emb.embed_texts(["can I use this offline"])[0]

hits = db.retrieve(qvec, 5, expected_model_hash=emb.model_hash(), query_text="can I use this offline")
print(hits[0].citation_id, hits[0].score, hits[0].text)
```

<details>
<summary>Search variants, validate, build</summary>

```python
db.search(qvec, 5)                                     # exact
db.search_ann(qvec, 5, 400)                            # hnsw beam (floor: the build's ef_construction), then exact rerank
db.search_hybrid(qvec, "vacina contra covid", 5, 100)  # bm25 + vectors, exact rerank
db.search_graph(qvec, 5, hops=2, ef=100)               # chunk graph from the seeds
db.search_space("clip-vit-b32", ivec, 5)               # one named multimodal space

assert db.validate() is True
info = db.inspect()
```

Each chunk is a dict with `canonical_text`, `source_uri`, `byte_start`, `byte_end` and `embedding`:

```python
urna.build(
    output_path="my_corpus.urna",
    embedding_model="sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
    embedding_dim=384,
    chunker_version="fixed-512/1",
    model_hash=model_hash,
    chunks=chunks,
    reproducible=True,
    preset="hybrid",
)
```

`python examples/quickstart/quickstart.py` runs the whole loop, build to cited hits, with no network.

</details>

## CLI

The engine verbs take a file and a vector; two of them run python (`search-text` for its embedder, `doctor` to probe the environment), the other ten never do. The agent verbs (`ask`, `retrieve`, `build`) take text and use the offline embedder; `ask` and `retrieve` search by what the file carries: bm25 plus vectors when the file has a bm25 index, hnsw when it has one, exact otherwise. `setup` and `tui` are the terminal ui. `urna --help` lists all three groups.

<details>
<summary>Agent verbs</summary>

```sh
urna ask my_corpus.urna "can I use this offline" -k 3
```

```sh
urna retrieve my_corpus.urna "can I use this offline" -k 5 --format jsonl
```

```sh
urna build --spec corpus.toml --dry-run
```

`build` reads one toml: the source (sqlite, csv, jsonl, an image dir), the media settings, and one or more embedding models from the registry (`potion`, `clip-vit-b32`, `siglip2`, `wemm-2b`, ...). Each model becomes a named vector space in the same file. The full spec is in [usage section 13](https://github.com/hoffresearch/urna/blob/main/docs/USAGE.md).

</details>

<details>
<summary>Search</summary>

```sh
urna search my_corpus.urna "[0.1, 0.2, ...]" -k 10
```

```sh
urna search-ann my_corpus.urna "[0.1, 0.2, ...]" -k 10 --ef 800   # the beam floor is the build's ef_construction (400)
```

```sh
urna search-graph my_corpus.urna "[0.1, 0.2, ...]" -k 10 --hops 2 --ef 100
```

```sh
urna search-space my_corpus.urna "[0.1, ...]" --space "wemm-2b@256" -k 5
```

```sh
urna search-text my_corpus.urna "vacina contra covid funciona" -k 5
```

</details>

<details>
<summary>Inspect, validate, stats, cite, media, benchmark, doctor</summary>

```sh
urna inspect my_corpus.urna --json
```

```sh
urna validate my_corpus.urna
```

```sh
urna stats my_corpus.urna
```

```sh
urna cite my_corpus.urna 'urna://sha256:1aa9.../sha256:8f314...'
```

```sh
urna media my_corpus.urna --export DIR
```

```sh
urna benchmark my_corpus.urna -q 100 -k 10 --ann 100 --madvise-cold
```

```sh
urna doctor
```

</details>

## Benchmarks

100,000 x 384 rows, k=10, one thread, same machine for every store.

| Store | p50 (ms) | p99 (ms) | Cold open (ms) |
|-------|---------:|---------:|---------------:|
| hnswlib | 0.32 | 0.53 | 182 |
| usearch | 0.67 | 61.4 | 58 |
| urna hybrid | 0.72 | 1.02 | 356 |
| urna exact | 7.80 | 8.32 | 292 |
| lancedb | 16.7 | 19.4 | 612 |
| sqlite-vec | 19.8 | 24.8 | 50 |

Both urna rows return recall@10 = 1.000. Urna's cold open includes checking every section hash before the first answer. Urna does not do updates, filters or concurrent writers. Method and the full table: [docs/BENCH.md](https://github.com/hoffresearch/urna/blob/main/docs/BENCH.md).

<details>
<summary>Presets: size vs recall</summary>

| Preset | Embeddings | Index | Size | Recall@10 |
|--------|------------|-------|-----:|----------:|
| `exact` | float32 | | 1.000 | 1.000 |
| `compressed` | float16 | | 0.339 | 1.000 |
| `tiny` | int8 | hnsw | 0.256 | 0.992 |
| `micro` | mrl256-int8 | hnsw | 0.223 | 0.810 |
| `nano` | int4 | hnsw | 0.209 | 0.913 |
| `hybrid` | float32 | hnsw + bm25 | 0.609 | 1.000 |

Measured on a 30,725-chunk pt-br corpus. Recall here is rank stability under quantization, not real-query quality. Details in [usage section 6](https://github.com/hoffresearch/urna/blob/main/docs/USAGE.md).

</details>

<details>
<summary>Images: 38,627 magic cards in one file</summary>

| Profile | Media | File | Vs the jpeg source |
|---------|-------|-----:|-------------------:|
| `archive` | JPEG XL, byte-reversible | 3.61 GB | 1.10x |
| `stills` | AV1 all-intra crf35 | 1.37 GB | 2.89x |
| `retrieval` | AV1 all-intra crf50 | 533 MB | 7.46x |

Text-to-image hit@1 over every card: siglip2 0.750, wemm-2b 0.744, jina 0.336, clip 0.098. Code and data: [mtg-urna-benchmark](https://github.com/brennercruvinel/mtg-urna-benchmark).

</details>

## Reference

- [docs/USAGE.md](https://github.com/hoffresearch/urna/blob/main/docs/USAGE.md): every verb, presets, models, builds, install channels
- [docs/BENCH.md](https://github.com/hoffresearch/urna/blob/main/docs/BENCH.md): how the numbers were measured
- [docs/SECURITY.md](https://github.com/hoffresearch/urna/blob/main/docs/SECURITY.md): reporting, hardening, data governance
- [docs/CHANGELOG](https://github.com/hoffresearch/urna/blob/main/docs/CHANGELOG): releases with measured numbers
- [docs/arc/ARC.toml](https://github.com/hoffresearch/urna/blob/main/docs/arc/ARC.toml): the architecture map
- [AGENTS.md](https://github.com/hoffresearch/urna/blob/main/.contracts/.agents/AGENTS.md): notes for contributors and agents

The crates are `urna-format` (the container), `urna-runtime` (search), `urna` (the binary) and `urna-python` (the bridge).

> Renamed from `nest` after 0.4.0. A `.nest` file written by 0.4.0 still opens.

## License

MIT, see [docs/LICENSE](https://github.com/hoffresearch/urna/blob/main/docs/LICENSE). [Hoff Research](https://hoffresearch.com)

Made it simple, but significant (∂μfμν = jν)

Author: Brenner Cruvinel
