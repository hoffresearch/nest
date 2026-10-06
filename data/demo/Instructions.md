---
project: urna
audience: contributors working with the demo data
status: active
last-updated: 2026-10-02
domain: data
---

# Demo

Local data behind the corpora `urna` measures itself on: the pt-BR fake-news corpus of the regression gate, and the image sources of the image benchmark. Nothing in here is required to use `urna` itself; `urna` reads `.urna` files and only `.urna` files.

This directory is local-only and gitignored, except this file.

## Contents

```
data/demo/
├── truw-built/                  v2 canonical csvs, npy embeddings and the sqlite-era truw_ptbr.urna (rust/bridge/python/urna/entry/oldformat.py)
├── derm/ph2 + derm/ham10000     dermoscopy images for the image benchmark
├── wsi/CMU-1.svs + wsi/cmu1-tiles   whole-slide scan and derived tiles
└── pdf/birdcraft-1907.pdf       scanned book, public domain
```

## The fake-news corpus

`data/corpus_next.v1.urna` (Git LFS) is the file the regression gate measures: `presetrun.py` rebuilds it at the other presets and `fullcheck.sh` compares the numbers with `data/measure/baseline.json`. The gate reads the file and never rebuilds it from the datasets.

| Field | Value |
|---|---|
| file_hash | `sha256:4229b7b3abfb85ddd75ebf183e0518acf60b8c4c7816c2f1df4c9041ef9b3233` |
| content_hash | `sha256:0ef1cf8f5d682f4614a0c4ea38f093a432c352de3a350362d09fcef2ff05917e` |
| Chunks | 30,725, one per text |
| Model | `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2`, 384d, float32, `exact` preset |

`shasum -a 256 data/corpus_next.v1.urna` checks the first line; `urna inspect --json` reads both hashes from inside the file.

The file is frozen. It was built from seven pt-BR fake-news datasets that can no longer be fetched in the form its loader read: the vzani hub datasets replaced their CSVs with Parquet, FakeRecogna replaced its CSV with XLSX, and the `opit-research/factck-br` mirror is gone. Two things in its content are known and left as they are, because the gate measures rank stability under quantization and neither changes that: the 7,200 Fake.br articles are in it twice (once from the vzani copy, once from Fake.br's stopword-stripped `pre-processed.csv`), and the FACTCK.BR labels came from the numeric rating, which marks 469 claims rated "falso" as true.

The corpus is rebuilt, with every source pinned to a revision and a tree hash, in [fakenews-ptbr-urna-benchmark](https://github.com/brennercruvinel/fakenews-ptbr-urna-benchmark): the fetch, the normalization and dedup (23,335 documents), the queries with TREC qrels, builds with three example embedders and the evaluation. Its `docs/sources.md` is the license bill of materials of the seven sources. The built corpus and its `.urna` files are in the [dataset of the same name](https://huggingface.co/datasets/brennercruvinel/fakenews-ptbr-urna-benchmark).

The corpus embeds political and health claims about named public figures; see the data governance section of [`docs/SECURITY.md`](../../docs/SECURITY.md#data-governance). For anything you ship broadly, prefer the CC0 `demo/corpora/intro`.

## Image corpora

`data/demo/derm/` holds the dermatology images used to measure the image corpus path. Like everything else here it is local-only and gitignored.

```
data/demo/derm/
├── ph2/images/              200 dermoscopy images + PH2_simple_dataset.csv (diagnosis labels)
└── ham10000/images/         10,015 dermatoscopic images + labels.csv (dx labels), phase 6 used a 2000-sample seed 42
data/demo/wsi/
├── CMU-1.svs                aperio whole-slide scan (openslide test data)
└── cmu1-tiles/              1210 tiles rendered from CMU-1.svs
data/demo/pdf/
└── birdcraft-1907.pdf       508-page scanned book, public domain (published 1907)
```

Four sources, four media regimes, which is what makes the measurement honest:

- PH2 (Mendonca et al., ADDI project, Universidade do Porto): 200 dermoscopy images labelled Common Nevus, Atypical Nevus, or Melanoma. Small, labelled, visually homogeneous. Research-use only, check the upstream terms before redistributing.
- HAM10000 (Tschandl et al., Harvard Dataverse, doi:10.7910/DVN/DBW86T): 10,015 dermatoscopic images across seven diagnostic classes. CC BY-NC 4.0, so any derived corpus is non-commercial and must carry attribution.
- CMU-1.svs (OpenSlide test data, Carnegie Mellon): one whole-slide scan tiled into 1210 tiles. Freely redistributable test data; the tiles are a derived artifact produced here.
- birdcraft-1907 (Wright, "Birdcraft", 1907): scanned book in the public domain; the PDF pages are the image items.

Rebuild the benchmark (the control index is not optional: the compressed numbers mean nothing without it):

```sh
.venv/bin/python rust/bridge/python/urna/entry/imgcorpus.py \
    --input-dir data/demo/derm/ph2/images --dataset ph2 \
    --output tmp/ph2/ph2.urna --labels data/demo/derm/ph2/PH2_simple_dataset.csv
.venv/bin/python rust/bridge/python/urna/entry/imgcorpus.py \
    --input-dir data/demo/derm/ph2/images --dataset ph2 \
    --output tmp/ph2-control/ph2-control.urna --labels data/demo/derm/ph2/PH2_simple_dataset.csv \
    --control
.venv/bin/python tool/bench/imageeval.py \
    --index tmp/ph2/ph2.urna --baseline tmp/ph2-control/ph2-control.urna -k 1 5 10
```

The full variant matrix (AV1 CRF ladder, avif444, control, dtype rungs, ordering) is one command per dataset with `tool/bench/imagerate.py`; see `docs/USAGE.md` for the flags and `docs/CHANGELOG` for the measured matrix with confidence intervals.

## Offline demo (no downloads)

None of the data in this directory is needed for the one-GIF demo. `python rust/bridge/python/urna/reads/retrieval.py` builds a byte-identical `.urna` from the CC0 demo corpus in `demo/corpora/intro` using the vendored potion embedder (numpy + tokenizers, no torch, no network) and prints a cited answer in seconds. It only needs `git lfs pull` to hydrate the potion table.

## Licenses

Each fake-news source carries a license, MIT or Apache-2.0: four declare it upstream, and for three it rests on the maintainer's verification, whose evidence is still to be recorded (`license_evidence` in the benchmark's `sources/sources.toml`), so their redistribution is not established yet. Its `docs/sources.md` lists each source, the basis for its license and the attribution it asks for.

The image sources carry their own terms: PH2 is research-use only (ADDI project, Universidade do Porto), HAM10000 is CC BY-NC 4.0 (Tschandl et al., doi:10.7910/DVN/DBW86T, non-commercial with attribution), CMU-1.svs is freely redistributable OpenSlide test data, and birdcraft-1907 is public domain. A `.urna` derived from PH2 or HAM10000 is therefore non-commercial and attribution-carrying at best; do not ship one as a product artifact.
