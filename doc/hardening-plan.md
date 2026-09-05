# hardening plan

working list for taking `nest` from "the agent did what was asked" to "a
reviewer finds nothing in the first hour". source: an external review of the
v0.3 tree (2026-09-04) that named eight concrete weaknesses and one strategic
one. this file tracks each item to a verifiable state: a command that proves
it, or a decision that closes it. status as of 2026-09-05.

## 1. review items

| # | finding | status | proof |
|---|---|---|---|
| 1 | `vec![0.0; dim]` per call inside the int4 SIMD kernel (avx2 + neon), plus a `Vec` per candidate for the int4 scales: thousands of malloc/free pairs per rerank | done | `dot_f32_i4_blocked(.., scratch: &mut [f32])`, `Int4EmbeddingsView::row_scales_into`, `RerankSource` owns both buffers; `grep -rn 'vec!\[' crates/nest-runtime/src/simd/` is empty; bit-identical scores asserted by the harness in `crates/nest-runtime/examples/` run (see §3) |
| 2 | `debug_assert_eq!` at the safe/unsafe boundary of the SIMD dispatcher: absent from release, the raw-pointer kernels trusted a validator three layers away | done | every dispatcher in `simd/mod.rs` uses `assert!`/`assert_eq!` (release-kept); `grep -n debug_assert crates/nest-runtime/src/simd/mod.rs` matches only the module doc that explains the rule |
| 3 | zero `// SAFETY:` on 40 `unsafe` blocks in a project selling "verifiable" | done | 31 `unsafe` sites remain (SIMD kernels + 2 mmap calls), each with `// SAFETY:` and each `unsafe fn` with `# Safety`; `clippy::undocumented_unsafe_blocks = "deny"` in `[workspace.lints]` makes a regression a build error. the 9 removed sites (header/footer/section-entry byte views, int8 row view) are `bytemuck` casts whose layout the derive verifies at compile time |
| 4 | `unwrap()` in 32 src files of a runtime that opens third-party files | done | `clippy::unwrap_used = "deny"` workspace-wide (tests exempt via `clippy.toml`); parse paths read fields through `nest_format::bytes::{le_u32, le_u64, le_f32, array32}` (typed `UnexpectedEof`); `cargo clippy --workspace --all-targets -- -D warnings` is clean |
| 5 | `/// l` artefact on doc comments | done | HEAD had 264 `///`, the tree also had 200 `//` and `#` sites across 55 files; `grep -rnE '(//|#) l[A-Z]' crates python` is empty. a stray mid-sentence `lAll` in `reader/validate.rs` went with them |
| 6 | no fuzzing; 12 hand-written negative tests | done | deterministic mutation harnesses on stable, run by `cargo test`: `crates/nest-format/tests/mutation_fuzz.rs`, `crates/nest-runtime/tests/mutation_fuzz.rs`; soaked at 62,500 x 4 fixtures (reader) and 10,000 x 4 (runtime) clean after the fixes in §2. `fuzz/` has four `cargo-fuzz` targets + seeds; `ci.yml` smoke-runs them on nightly. coverage-guided soak run 2026-09-05 (nightly installed via rustup for it): numbers per target in §3; it found finding #5 |
| 7 | stale "NOT yet implemented" comments in `layout/mod.rs` | done | the section-id map now states per-id status (0x09 read-only, 0x0A/0x0B via the text codec, 0x0C and 0x14-0x17 shipped, 0x0D-0x13 names only) |
| 8 | CLI scope: 17 subcommands incl. `ask`/`retrieve`/`cite`/`embed_gate` in one 2,174-line binary | done (decision: one binary, two groups) | `nest --help` lists the engine verbs first and the agent verbs last, each summary tagged `[engine]` / `[agent]`, a legend in the footer; the tree mirrors it (`cmd/*.rs` vs `cmd/agent/{ask,retrieve,build}.rs`); `embed_gate` and `pyenv` are shared helpers, not verbs; README shows the two groups. no verb renamed, no second binary (see §4.1 for why) |
| - | no CI ran `cargo test` on a pull request (only tag/release workflows) | done | `.github/workflows/ci.yml`: fmt, clippy deny lints, test on ubuntu (avx2) + macos (neon), mutation harnesses at higher count, 300-line guard, forge-core gate, ruff through the shared `scripts/ruff_check.sh`, cargo-fuzz smoke |

## 2. what the harness found on its first runs

five bug classes, each "a malformed `.nest` panics or aborts the runtime"
(in scope per doc/SECURITY.md), each fixed and pinned by a regression test.
none of the twelve hand-written negative tests covered them. four came from
the deterministic harness within its first 1,500 mutations; the fifth from
the coverage-guided run, after the deterministic harness had soaked clean.

1. **unchecked `n * dim * width`** in `expected_embeddings_size` (and the
   int8/int4 view parsers): overflow panic in debug, a wrapped product that
   could match a tiny section in release. now `checked_*` end to end;
   `tests/negative_header_overflow.rs`.
2. **NaN reaches a sort.** a NaN lane in a multimodal band (or the 0x09 fp
   slab) produced a NaN cosine; `partial_cmp(..).unwrap_or(Equal)` is not a
   total order and `sort_by` panics on it since rust 1.81. bands and the fp
   slab now pass the same finite-values gate as the canonical embeddings
   (`validate_slab_values`), and every runtime sort goes through
   `nest_runtime::order` (NaN-last total orders). BM25 additionally drops
   non-finite scores. `tests/space_band_nan.rs`.
3. **wrapping cursor bounds check** `pos + n > len` with an attacker-chosen
   `n` in every section cursor (format and runtime). all are `n > remaining`
   now; `tests/negative_bm25_payload.rs::huge_length_field_*`.
4. **BM25 postings never validated**: doc ids beyond `n_docs` indexed out of
   bounds at query time; non-finite `k1`/`b`/`avgdl` accepted. typed
   rejections at decode; `tests/negative_bm25_payload.rs`.
5. **unbounded count-driven allocation** (cargo-fuzz, `section_decoders`,
   90-byte input): the intpack spans repack sized its uri pool from `n_uris`
   before reading a byte, `Vec::with_capacity(1.3e9)` -> 31 GB malloc ->
   abort. bounded by `remaining / 4`; every other decode-path allocation was
   audited against the same rule (they already bounded, this one did not).
   `tests/negative_alloc_claims.rs` carries both libfuzzer artifacts.

pattern worth stating: every one of these lived one layer BELOW the
checksum layer. the twelve negative tests all corrupt then re-hash, but each
targets one field; the harness reseals half of its random mutations, which
is what reaches the decoders in bulk.

## 3. measured

- full suite: 355 tests green (`cargo test --workspace`), clippy clean with
  the deny lints, rustfmt clean, 300-line guard clean (three files I pushed
  over were trimmed; `nest_file.rs` was already over in the tree and lost
  `SearchHitPy` to `search_hit.rs`).
- mutation soak (release): 250,000 reader mutations + 40,000 runtime
  mutations, zero panics after the four fixes; before them the first 1,500
  found #1 and #2 within a second.
- coverage-guided fuzz (cargo-fuzz, libfuzzer + asan, nightly, 10 minutes per
  target, seeds from the harness fixtures), 2026-09-05 on this machine:

  | target | executions | result |
  |---|---|---|
  | `nest_view` | 47,639,167 | clean (cov 1106, corpus 861) |
  | `section_decoders` (before fix #5) | stopped early | oom: 31 GB `with_capacity` in the spans repack |
  | `section_decoders` (after fix #5) | 18,983,714 | clean (cov 2368, corpus 2096) |
  | `runtime_indexes` | 3,480,290 | clean (cov 948, corpus 413) |
  | `mmap_open_search` | 3,123,031 | clean (cov 3612, corpus 1569) |

  every run wrote its corpus under `fuzz/corpus/<target>/` (gitignored); the
  two libfuzzer artifacts of finding #5 are tracked as `fuzz/seeds/regress-*`.

- int4 rerank kernel, per-row allocation vs caller scratch: see the numbers
  appended at the end of this file (measured on this machine, 50k x 384).

## 4. open items, in priority order

### 4.1 CLI scope (decided 2026-09-05: one binary, two groups)

done as recommended below; kept here as the record of why. recommendation: keep ONE binary (a second binary doubles the installer,
homebrew formula, cargo-dist matrix and install-test surface for zero user
gain), but make the two products visible in the tool itself:

- `clap` `help_heading`s: `engine` (inspect, validate, stats, search,
  search-ann, search-graph, search-space, search-text, benchmark, cite,
  media, doctor) and `agent` (ask, retrieve, build). `embed_gate` is not a
  verb, it is a helper module; leave it out of the help.
- `crates/nest-cli/src/cmd/agent/{ask,retrieve,build}.rs` as a submodule so
  the file tree says the same thing the help says.
- README: two code blocks, one per heading, instead of one list of 14 verbs.

alternative if the maintainer wants the split anyway: `nest-cli` stays the
engine, `nest-agent` (new crate, same workspace) owns ask/retrieve/build and
depends on `nest-cli` as a library. cost: one more dist target and formula.

### 4.2 competitor benchmark for the README (done: `doc/benchmarks.md`)

shipped as `python/tools/bench_competitors.py` + `_bench_systems.py`, table in
`doc/benchmarks.md`, linked from the README. deviations from the spec below,
all deliberate: synthetic seeded rows instead of the LFS corpus (the LFS
budget is exhausted, and synthetic rows let anyone reproduce it without
data), no multimodal column (no competitor exposes one), the citation claim
is measured as raw-text vs zstd-text sharing one `content_hash` (index type
is part of the canonical search contract, so exact vs hybrid legitimately
differ). one finding the table makes visible: nest's HNSW build is roughly
20x slower than hnswlib at the same m / ef_construction (single-threaded,
no batching); item 4.11. original spec: nobody switches without a number. `python/tools/bench_competitors.py`, one
corpus, one machine, one table, checked in with the exact versions.

- candidates: `usearch` (the closest single-file mmap peer), `sqlite-vec`
  (the "embedded" default), `hnswlib` (the recall/latency reference),
  `lancedb` (the columnar single-dir peer). chroma/qdrant-embedded are
  services, not files; one line saying why they are out.
- corpus: `dat/measure/fakerecogna_exact.nest` source rows (already LFS
  tracked, PT-BR, ~10k) and the 38k-card image corpus for the multimodal
  column only where a competitor supports it.
- columns: build wall time; artefact bytes on disk; cold open to first
  query (process start -> first result, the mmap argument); exact-search
  p50/p99 at k=10; ann p50/p99 + recall@10 vs brute force; second build
  byte-identical? (`sha256` of two builds from the same input);
  integrity check available? (validate + what it proves); citation stable
  across re-encoding? (content_hash across raw/zstd/int8 twins).
- honesty rows, stated as rows not footnotes: nest has no in-place update,
  no metadata filtering, no concurrent writer; the competitors that do get
  a "yes".
- output: `doc/benchmarks.md` with the table + `bench/competitors/` spec and
  lockfile; README links the table and quotes two numbers (cold open, bytes
  on disk), nothing else.

### 4.3 coverage-guided fuzz soak (first run done, see §3; keep going)

first soak ran 2026-09-05, 10 minutes per target, and paid for itself with
finding #5. next: a nightly `schedule:` job in `ci.yml` (30 min per target,
artifacts uploaded on crash) and a one-hour local soak per target after any
decoder change. every finding becomes a `tests/negative_*.rs` before the fix.

### 4.4 miri on nest-format

the format crate now has zero `unsafe`, so `cargo +nightly miri test -p
nest-format` is cheap and turns "no UB" from a claim into a run. one CI job,
nightly schedule.

### 4.5 property tests for the codecs

`proptest` roundtrips for `intpack`, `txt_streams`, `fsst`, `zstd_dict`,
`dedup`, int8/int4 quantize+pack: `decode(encode(x)) == x` and
`encode(x)` byte-stable across two calls. these are the byte-identity
claims the citation URI depends on; today they are tested on fixed inputs
only.

### 4.6 write the 0x09 fp slab

the runtime reranks from `embeddings_fp` when present, but no writer emits
it, so an int4 corpus reranks at stored precision and every hit discloses
that. `NestFileBuilder::embeddings_fp(EmbeddingDType::Float16)` writes the
64-byte-aligned raw slab next to the int4 section; `nano` preset gains it;
measure_presets gets the recall delta. this closes the honesty-disclosure
gap instead of documenting it.

### 4.7 kernel benchmarks in-repo

`crates/nest-runtime/benches/simd.rs` (criterion): f32/f16/i8/i4 dot per
backend at dim 256/384/768, and the rerank loop over 50k rows. the
allocation fix in §1 was measured with a throwaway example; the next kernel
change should be measured by `cargo bench`, not asserted.

### 4.8 supply chain items already promised in SECURITY.md

- signed release tags (`git tag -s`, verify step in `release.yml`).
- SBOM per release (`cargo cyclonedx` -> attach to the release, attest it
  like the binaries).
- `cargo deny` (advisories + licenses) as a CI job.

### 4.9 public API discipline for nest-format

`cargo semver-checks` in CI on the format crate: the on-disk format is
frozen, the rust API that reads it should say when it breaks.

### 4.11 HNSW build throughput

measured in `doc/benchmarks.md`: at m=16 / ef_construction=200 nest builds
its graph ~20x slower than hnswlib and usearch on the same rows (single
thread, one insert at a time, `select_neighbors` re-sorting per candidate).
recall at ef=100 is higher than both, so the graph is fine; the build loop
is not. plan: profile `ann/build.rs` (`cargo flamegraph` on a 100k build),
batch the level-0 inserts, parallelize the independent upper-layer searches
with rayon behind a deterministic merge (the seed contract must hold: same
input, same graph bytes), then re-measure with the same table. a `criterion`
bench (4.7) guards the number afterwards.

### 4.10 README positioning

- first line says what it is in plain words (single-file, memory-mapped,
  hash-verified vector database with stable citations), not "sovereign";
  the word can stay in the section that explains what the format enforces.
- differentiators first (byte-identical rebuilds, `nest://content_hash/
  chunk_id`, exact-cosine rerank on every path, offline by construction),
  then the benchmark table from 4.2, then install.
- one honest paragraph: this is infrastructure for document and clinical
  retrieval products that are not public yet; `data-governance.md` already
  says the hard part.
- badges that mean something: ci, fuzz (once 4.3 runs), no `unsafe` in
  nest-format, msrv.

## 5. verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
NEST_MUTATION_ITERS=25000 cargo test --release -p nest-format --test mutation_fuzz
NEST_MUTATION_ITERS=4000  cargo test --release -p nest-runtime --test mutation_fuzz
find crates -name '*.rs' -path '*/src/*' -not -path '*/tests/*' | xargs wc -l | awk '$1 > 300'
NEST_PYTHON=.venv/bin/python sh scripts/ruff_check.sh
```

## 6. int4 rerank kernel, measured 2026-09-05

throwaway example (deleted after the run), release build, 50,000 rows x
384 dims, 5 passes, scores asserted bit-identical between the two shapes:

```text
    backend=neon n=50000 dim=384 reps=5
    alloc-per-row:    152.7 ns/row (38.2 ms total)
    scratch-reuse:    107.4 ns/row (26.8 ms total)
    speedup: 1.42x
```

1.42x per row on neon (the allocation pair was ~30% of the per-row cost);
the scalar backend measured 1.18x (263.4 -> 223.7 ns/row). more on a glibc
allocator under contention. the point of the change is that the hot path
no longer depends on the allocator at all.
