# demo

raw and intermediate data used to build the truw corpus that ships with `nest`. each subdirectory is either a public PT-BR fake-news dataset (kept verbatim from its upstream distribution, license intact) or a derived artifact produced from those sources.

nothing in here is required to use `nest` itself. `nest` reads `.nest` files and only `.nest` files. this directory exists so that the corpus shipped with the project is reproducible: anyone can re-run `python python/tools/nest_build_corpus.py` and get a byte-identical `data/corpus_next.v1.nest`.

tracked via Git LFS. expect ~600 MB on first clone.

## contents

```
data/demo/
├── FakeBr-hf/                                            7.2k rows  (csv, train+test)
├── FakeTrue.Br-hf/                                       3.6k rows  (csv, train+test)
├── Fake.br-Corpus/                                       7.2k rows  (preprocessed + full_texts)
├── FakeRecogna/                                         11.9k rows  (csv + html report)
├── FakeTrue.Br/                                          3.6k rows  (single csv)
├── factck-br/                                            1.3k rows  (tsv from agência lupa)
├── portuguese-fake-news-classifier-bilstm-combined/      2.2k rows  (parquet, used as test split)
├── corpus-combined/                                       (merge of FakeTrue.Br + Fake.br, v2)
├── corpus-next/embed_cache.sqlite                        (sentence-transformers cache, ~63 MB)
└── truw-built/                                            v2 canonical CSVs + npy embeddings
```

each upstream dataset keeps its original license file and structure. see the README inside each subdirectory for citation requirements.

## what truw uses this for

truw is a fact-checking pipeline that scores brazilian-portuguese claims against a curated corpus of labeled news articles. the `.nest` file shipped with the application is rebuilt from these seven sources:

1. read each loader (see `python/tools/_corpus_sources.py`)
2. normalize to a common shape (`text, label, source, title, url`)
3. filter rows shorter than 20 characters
4. deduplicate by sha256 of the text, keep first occurrence
5. embed with `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` (dim 384, L2-normalized)
6. emit a deterministic `.nest` with the model fingerprint stamped in the manifest

the result is `data/corpus_next.v1.nest`: 30,725 deduped chunks, 119 MB at the `exact` preset, 34 MB at `tiny`. every chunk keeps a `corpus-next://<source>/<sha256>` provenance URI back to the dataset it came from.

`truw-built/` carries the v2 canonical artifacts for the older fact-check pipeline: `truw_canonical_ptbr_v2.csv` (23.5k articles with claim, label, source, category, date), pre-computed embeddings as `.npy`, and the original `truw_ptbr.nest`. see `truw-built/BUILD.md` for the schema and the v2 build recipe.

## how to use

### rebuild the unified corpus

```
python python/tools/nest_build_corpus.py
```

writes `data/corpus_next.v1.nest`. uses `data/demo/corpus-next/embed_cache.sqlite` so re-runs skip the embedding step. takes ~10 minutes from cold cache, seconds from warm.

with `reproducible=True` (the default in `BuildConfig` for this script) two operators on different machines produce byte-identical files. `file_hash` and `content_hash` will match.

### load one dataset directly in python

each loader returns a `pandas.DataFrame` with columns `text, label, source, title, url`:

```
import sys; sys.path.insert(0, "python/tools")
from _corpus_sources import load_fakebr_hf, load_factck_br, SOURCES

df = load_fakebr_hf()          # 7.2k rows from FakeBr-hf
df = load_factck_br()          # 1.3k rows from factck-br

# or iterate every source in the registry:
for name, loader in SOURCES:
    print(name, len(loader()))
```

### query the truw v2 corpus directly

`truw-built/truw_canonical_ptbr_v2.csv` is the human-readable canonical form. the `.nest` file equivalent (`truw-built/truw_ptbr.nest`) is opened the same way as any other `.nest`:

```
import sys; sys.path.insert(0, "python")
import nest

db = nest.open("data/demo/truw-built/truw_ptbr.nest")
hits = db.search(qvec, k=5)
```

## why these seven sources

each one fills a gap the others miss:

- `FakeBr-hf` and `Fake.br-Corpus` are the canonical public PT-BR fake-news baseline (UFG / USP work).
- `FakeTrue.Br` and `FakeTrue.Br-hf` add explicit true/fake pairs for the same claims, useful for label calibration.
- `FakeRecogna` is the largest single corpus (11.9k rows), covers politics + entertainment + health.
- `factck-br` carries 1.3k claims reviewed by agência lupa with their original ratings; high-confidence labels.
- `portuguese-fake-news-classifier-bilstm-combined` provides a held-out test split with a known baseline classifier.

deduplication by sha256 collapses the inevitable overlap (FakeRecogna re-publishes some fake.br articles, FakeTrue.Br shares headlines with FakeBr-hf, etc.) so the unified corpus has no double-counting.

## licenses

each subdirectory inherits its upstream license. some are CC-BY-SA, some are MIT, some are unspecified academic distributions. if you redistribute a built `.nest` derived from this directory, the license of the resulting corpus is the union of all upstream licenses (most restrictive wins). check each `README` inside the subdirectories.
