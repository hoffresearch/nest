# ADR 0003: SHA-256 as the only hash function

- **Status:** Accepted
- **Date:** 2026-04-27
- **Deciders:** project owner

## Context

`.nest` files carry six distinct hashes:

- header checksum (8-byte truncation)
- per-section checksum (8-byte truncation)
- footer file hash (full 32 bytes)
- canonical content hash (full 32 bytes)
- chunk_id (full 32 bytes, surfaced as `sha256:<hex>`)
- model_hash (caller-supplied `sha256:<hex>`)

Mixing two algorithms (e.g. SHA-256 for some, BLAKE3 for others) would
force every consumer to track which is which.

## Decision

Use SHA-256 for every hash in v1. No BLAKE3, no SHA-1, no xxhash.
`model_hash` is `sha256:` prefixed, and the format-internal checksums
all derive from `sha2::Sha256::digest`.

## Consequences

### Positive

- One implementation across `nest-format`, `nest-runtime`, the CLI,
  the Python bindings, and `chunk_id` derivation.
- Cryptographically conservative: SHA-256 collision resistance is the
  industry baseline.
- Every `*_hash` field a user sees has the form `sha256:<64 hex>`.
  Trivially recognizable.

### Negative

- BLAKE3 would be ~3× faster on large blobs (e.g. the 30 MB
  embeddings section). Build time and `nest validate` are
  sub-second on the 73 MB legacy dataset, so the cost is bounded.

### Trade-offs

- The 8-byte truncations in the header and per-section checksums are
  **detection-grade**, not authentication-grade — they catch
  bit-rot/corruption, not adversarial tampering. The full 32-byte
  footer `file_hash` is the authentication boundary.

## Alternatives considered

- **BLAKE3 primary, SHA-256 only for `chunk_id` / `model_hash`.**
  Rejected: split implementations, no real win at our throughput.
- **SHA-1 truncation for the header checksum (a la zip).** Rejected:
  SHA-1 is deprecated; consistency wins.

## References

- `crates/nest-format/src/layout.rs` (header_checksum, section_checksum)
- `crates/nest-format/src/reader.rs` (footer hash, content_hash)
- `crates/nest-format/src/chunk.rs` (chunk_id)
- `doc/spec.md` §8 (hashing summary)
