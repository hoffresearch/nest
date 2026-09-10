# benchmarks

measured 2026-09-10 on arm64 Darwin 25.6.0, python 3.12.14, single thread, n=100,000 synthetic clustered l2-normalized rows x 384 dims (2000 centers), 200 queries, k=10, seed 7. reproduce: `.venv/bin/python python/tools/bench_competitors.py --n 100000 --dim 384 --queries 200`.

| system | path | build (s) | bytes on disk | cold open + 1st query (ms) | p50 (ms) | p99 (ms) | recall@10 | rebuild byte-identical | integrity check |
|---|---|---|---|---|---|---|---|---|---|
| nest (exact) | exact | 2.14 | 165,290,134 | 292.3 | 7.803 | 8.315 | 1.0 | yes | yes (sha256 per section + file + content) |
| nest (hybrid) | ann (hnsw) | 172.82 | 163,221,498 | 356.1 | 0.721 | 1.021 | 1.0 | yes | yes (sha256 per section + file + content) |
| usearch | ann (hnsw) | 109.26 | 168,453,808 | 58.1 | 0.672 | 61.422 | 0.995 | yes | no |
| hnswlib | ann (hnsw) | 83.07 | 168,449,236 | 181.5 | 0.324 | 0.525 | 1.0 | yes | no |
| sqlite-vec | exact | 0.81 | 156,606,464 | 50.0 | 19.762 | 24.836 | 1.0 | yes | structural only (pragma integrity_check) |
| lancedb | exact | 0.25 | 153,799,983 | 612.4 | 16.728 | 19.401 | 1.0 | no | no |

how to read it:

- `cold open + 1st query`: wall time of a fresh interpreter that opens the store and answers one query, minus an interpreter doing nothing (3 runs, min). nest's number is dominated by `open` verifying every section checksum and the footer hash over the whole file before serving anything; the other stores trust their bytes.
- `build (s)`: single-threaded everywhere (hnswlib and usearch are told threads=1); nest's hnsw build is the slow row, tracked as doc/hardening-plan.md item 4.11.
- `p50 / p99`: warm, single-threaded, one query at a time, from python. python call overhead is inside every number.
- `recall@k` is against brute force over the same rows; exact paths are asserted at 1.0.
- `rebuild byte-identical`: two builds from the same rows compared by sha256 over the artefact (a directory is hashed file by file).
- `integrity check`: whether the store can prove its own bytes. nest verifies sha256 per section, per file and over the decoded content on `validate()`.
- the same rows written with raw text and with zstd text share one `content_hash`: True. re-encoding never moves a `nest://content_hash/chunk_id` citation; the other stores have no equivalent notion.

what nest does NOT do that some of these do: in-place updates or deletes, metadata filtering, concurrent writers, a query language. it is a build-once, ship-and-query file; the table says nothing about workloads that need those.

versions: {"usearch": "2.26.2", "hnswlib": "0.8.0", "sqlite-vec": "0.1.9", "lancedb": "0.38.0", "numpy": "2.5.2"}
