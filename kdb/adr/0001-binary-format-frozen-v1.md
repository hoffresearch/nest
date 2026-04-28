# ADR 0001: Freeze the .nest binary container at v1

- **Status:** Accepted (amended 2026-04-28: see ADR 0006 for active
  encodings 1/2/3 and ADR 0007 for optional sections 0x07/0x08)
- **Date:** 2026-04-27
- **Deciders:** project owner

## Context

`.nest` v1 needed a single, byte-stable container layout before any
runtime, CLI or Python binding could claim a stable contract. Without
freezing the bytes, `file_hash` and `content_hash` are not citation
material — they would shift with every refactor.

## Decision

Freeze the on-disk container at v1 with the following invariants:

- 4-byte magic `"NEST"`, `version_major = 1`, `version_minor = 0`,
  `format_version = 1`, `schema_version = 1`.
- 128-byte header (`repr(C)`, compile-time `assert_eq!(size_of, 128)`).
- 32-byte section table entry with fields
  `section_id(u32) | encoding(u32) | offset(u64) | size(u64) | checksum[8]`.
- 40-byte footer (`u64 footer_size = 40` + 32-byte SHA-256 file hash).
- Six required sections (chunk_ids, chunks_canonical,
  chunks_original_spans, embeddings, provenance, search_contract); v1
  rejects any file that omits or adds others.
- Every section's `offset` is a multiple of `SECTION_ALIGNMENT = 64`.
  Padding before each section is zero and is **excluded** from the
  section's checksum (but included in the file hash).
- Section payload `encoding` accepts only `0 = raw` in v1; values 1, 2,
  3 are reserved for `zstd`, `float16`, `int8` futures.
- All multi-byte integers are little-endian, unsigned unless noted.
- Reader rejects `format_version` or `schema_version` larger than its
  own constants; smaller-or-equal is accepted.

Any future change that alters the bytes on disk must bump
`NEST_FORMAT_VERSION`. Manifest-only schema additions bump
`NEST_SCHEMA_VERSION`.

## Consequences

### Positive

- `file_hash` and `content_hash` are durable identifiers; citations
  (`nest://content_hash/chunk_id`) survive rebuilds of the same
  content.
- The reader is exhaustive: magic, header checksum, alignment,
  encoding, section bounds, section checksum, footer hash, manifest
  schema, search-contract coherence, embeddings byte size, embedding
  values, required-section presence — all checked before
  `NestView::from_bytes` returns `Ok`.
- 64-byte alignment lets the embedding section be loaded directly via
  mmap into SIMD-friendly buffers without a memcpy.

### Negative

- Section padding inflates files by up to 63 bytes per section. For
  the converted 19,769-chunk dataset the overhead is 5 × ≤63 = 315 B
  out of 73 MB. Negligible.
- `format_version` is the only knob; minor incompatible tweaks have to
  ship as v2.

### Trade-offs

- Did **not** ship per-section compression in v1 (encoding=0 only).
  Reserving 1/2/3 lets v2 add zstd/fp16/int8 without another container
  bump.
- Did **not** add ANN section IDs in v1; recall=1.0 exact baseline must
  exist as ground truth before ANN can be benchmarked.

## Alternatives considered

- **Embed compression now (zstd everywhere).** Rejected: lossy paths
  are valuable but must be opt-in, not the default. Encoding-1 is
  reserved.
- **Use msgpack/CBOR for the manifest.** Rejected: JCS-canonical JSON
  preserves human readability and is hashable as text without a third
  dependency.
- **Hash each section with BLAKE3 for speed.** Rejected: SHA-256 is
  ubiquitous, cryptographically conservative, and the file hash is
  computed once per build/open. See ADR 0003.

## References

- `crates/nest-format/src/layout.rs` (constants and structs)
- `doc/spec.md` §§ 3-9 (byte-by-byte layout)
- `crates/nest-format/tests/golden.rs` (golden fixture)
