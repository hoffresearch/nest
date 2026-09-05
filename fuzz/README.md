# fuzzing

coverage-guided fuzzing of every byte-level entry point in `nest-format` and
`nest-runtime` with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
(libFuzzer + AddressSanitizer). the contract is doc/SECURITY.md: a malformed
`.nest` may be rejected with a typed error, never with a panic, a hang, or an
out-of-bounds read.

the deterministic twins of these targets run on stable under plain `cargo
test` (`crates/nest-format/tests/mutation_fuzz.rs`,
`crates/nest-runtime/tests/mutation_fuzz.rs`), so every CI run already
executes a few thousand mutations; this directory is the long soak.

## targets

| target | entry point | what a crash means |
|---|---|---|
| `nest_view` | `NestView::from_bytes` + every section decoder + hashes | reader / decoder bug |
| `section_decoders` | one section codec picked by the first byte, fed the rest | codec bug reachable behind a valid container |
| `runtime_indexes` | `HnswIndex::from_bytes`, `Bm25Index::from_bytes`, `CsrIndex::from_bytes` | index codec bug |
| `mmap_open_search` | file on disk, `MmapNestFile::open` + every search verb | runtime open / search bug |

`nest_view` and `mmap_open_search` reseal half of their inputs (recompute
the header checksum, section checksums and footer hash) so the fuzzer gets
past the integrity layer and into the decoders; the other half tests the
integrity layer itself.

## run

```sh
cargo install cargo-fuzz          # needs a nightly toolchain
cd fuzz
mkdir -p corpus/nest_view && cp seeds/*.bin ../crates/nest-format/tests/fixtures/golden_v1_minimal.nest corpus/nest_view/
cargo +nightly fuzz run nest_view -- -max_total_time=600
cargo +nightly fuzz run section_decoders -- -max_total_time=600
cargo +nightly fuzz run runtime_indexes -- -max_total_time=600
cargo +nightly fuzz run mmap_open_search -- -max_total_time=600 -rss_limit_mb=4096
```

a finding lands in `artifacts/<target>/`; reproduce with `cargo +nightly fuzz
run <target> artifacts/<target>/<file>` and turn it into a negative test
under `crates/*/tests/` before fixing.

## seeds

`seeds/*.bin` are real `.nest` files in every dtype / text-encoding
combination with every optional section present. regenerate them from the
stable harness:

```sh
mkdir -p fuzz/seeds
NEST_FUZZ_SEED_DIR=$PWD/fuzz/seeds cargo test -p nest-format --test mutation_fuzz
NEST_FUZZ_SEED_DIR=$PWD/fuzz/seeds cargo test -p nest-runtime --test mutation_fuzz
```

`ci.yml` runs every target for a short bounded time on each push (smoke,
not soak) with these seeds as the corpus.
