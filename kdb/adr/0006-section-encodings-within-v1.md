# ADR 0006: Section encodings 1/2/3 ship within v1

- **Status:** Accepted
- **Date:** 2026-04-28
- **Deciders:** project owner

## Context

ADR 0001 froze v1 with `encoding = 0` (raw) only and reserved values
1, 2, 3 for "future" zstd / float16 / int8 implementations. Field
work showed three things:

1. PT-BR text sections (chunks_canonical, original_spans, provenance,
   search_contract) compress 3-5× under zstd. Files in the 70–120 MB
   range drop to 20–40 MB at zero recall cost.
2. Float16 embeddings halve the on-disk vector section without
   measurable recall@10 loss when SIMD accumulates in f32.
3. Int8 with a per-vector f32 scale quarters the embedding section.
   recall@10 drops to ~0.97 — fine when paired with an exact rerank.

Bumping `format_version` to v2 to ship these would invalidate every
v1 file's `file_hash` reference and force readers to maintain two
container layouts.

## Decision

Wire encodings 1, 2, 3 inside v1 without bumping `format_version`:

- `encoding=1` (`zstd`) is valid for any non-embedding section. The
  reader decompresses transparently. Section checksum still hashes the
  **physical (compressed)** bytes.
- `encoding=2` (`float16`) is valid only for the embeddings section.
  The manifest must declare `dtype = "float16"`.
- `encoding=3` (`int8`) is valid only for the embeddings section. The
  manifest must declare `dtype = "int8"`. Layout: `u32 payload_version
  | u32 scale_kind | f32 scale[n] | i8 vec[n*dim]`.

`content_hash` continues to hash the **decoded** payload, so a corpus
stored with zstd has the same `content_hash` as the same logical
content stored raw. Citations stay stable across encoding choices.

Embeddings are never zstd-compressed — they live in mmap and are read
straight by the SIMD dot product.

## Consequences

### Positive

- v1 files keep their identity. `file_hash` of an existing
  `corpus_next.v1.nest` is still `file_hash` after the format
  acquires three new encodings.
- Citations work across compression choices for free.
- Compressed presets can be distributed at 0.28-0.35× the size of
  exact, opening up bandwidth-constrained delivery (offline drops,
  embedded devices).

### Negative

- ADR 0001's "encoding=0 only in v1" sentence is now stale; this
  ADR amends it.
- Readers older than 2026-04-28 will reject any file with encoding ≠ 0
  with `UnsupportedSectionEncoding`. That's the explicit, typed
  rejection the format contract promises — no silent corruption — but
  it does mean older binaries cannot read newer files. We accept this
  as a forward-compat issue, not a backward-compat one.

### Trade-offs

- Did **not** bump `format_version` to v2. Bumping breaks every
  existing reference to v1 hashes. Within-v1 encoding additions hit
  the same forward-compat cost without the citation-invalidation
  blast radius.

## Alternatives considered

- **Bump to v2.** Rejected: invalidates every v1 reference. Forward-
  compat cost is the same.
- **Use a flags field for compression.** Rejected: the encoding slot
  in the section table is already 4 bytes and tagged per-section,
  which is more granular than a file-level flag.

## References

- `doc/spec.md` §5 (encoding values), §6.5 (embeddings layouts).
- `crates/nest-format/src/encoding/` (split: zstd_codec / float16 / int8).
- `crates/nest-format/src/reader/validate.rs::validate_encoding_for_section`.
- ADR 0001 (frozen v1 container).
