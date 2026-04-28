# ADR 0007: HNSW (0x07) and BM25 (0x08) ride as optional sections

- **Status:** Accepted
- **Date:** 2026-04-28
- **Deciders:** project owner

## Context

ADR 0001 set the v1 contract at six required sections (chunk_ids,
chunks_canonical, chunks_original_spans, embeddings, provenance,
search_contract) and rejected files with extra section IDs.

Production telemetry showed two needs the exact path can't satisfy
alone:

1. **ANN for the latency floor.** Exact flat search at p50 ≈ 3 ms is
   already fast for 30k chunks but linear in `n`. ANN drops `tiny`
   preset to p50 < 1.5 ms.
2. **Lexical recall for rare terms.** PT-BR queries with proper
   nouns, siglas, and abbreviations under-rank against pure
   embeddings. BM25 candidates fused with vector candidates (RRF)
   restore recall on those queries.

Embedding ANN graphs and BM25 indexes inside the embeddings section
or inside provenance is wrong: they have their own version, their own
size guarantees, and their own validation rules.

## Decision

Reserve two new section IDs as **optional**:

- `0x07 = hnsw_index` — present when `manifest.capabilities.supports_ann
  = true`. Pure-Rust HNSW graph (Algorithm 4 heuristic), bit-equal
  across rebuilds with the same seed. Always `encoding = 0` (raw).
- `0x08 = bm25_index` — present when `manifest.capabilities.supports_bm25
  = true`. Versioned posting lists, deterministic alphabetical term
  order. Honors `text_encoding` (raw or zstd).

These sections are NOT in `CANONICAL_SECTIONS`. They do NOT participate
in `content_hash`. Adding HNSW or BM25 to a corpus does not invalidate
any existing citation. The same logical content with or without these
auxiliary indexes shares the same `content_hash` and therefore the
same `nest://content_hash/chunk_id` URIs.

Both paths **always** rerank candidates with the exact dot product
before returning. The final score is the real cosine value. ADR 0005
was strict about exact-only because there was no rerank story; this
ADR is consistent with that constraint.

## Consequences

### Positive

- Existing corpora can be upgraded to ANN/hybrid in place — copy the
  file, append the optional section, content_hash is unchanged.
- The reader contract is conditional, not branching: readers that
  don't know about 0x07/0x08 simply ignore them (we accept "extra
  sections" only when their IDs are in `OPTIONAL_SECTIONS`).
- recall remains a contract: ANN/hybrid candidates always go through
  exact rerank, so the `score` field is honest cosine, not an ANN
  proxy.

### Negative

- Older readers (pre-2026-04-28) will reject files containing 0x07 or
  0x08 with `MissingRequiredSection` because they only know the six
  required IDs and will see "unknown" sections. We accept this as
  forward-compat; ADR 0006's reasoning applies.

### Trade-offs

- ANN candidate selection sits in the runtime crate, not the format
  crate. The format crate stays I/O-only; the runtime owns search math.
  This is cleaner but means HNSW recall measurements live in
  `crates/nest-runtime/tests/hnsw_recall.rs`, not next to the rest of
  the format roundtrips.

## Alternatives considered

- **Embed HNSW into the embeddings section.** Rejected: pollutes the
  recall=1.0 ground truth with ANN-shaped data; complicates section
  checksums.
- **Add ANN as a separate file alongside the .nest.** Rejected:
  citation URIs depend on a single `content_hash`; spreading payload
  across files breaks the single-file distribution promise.

## References

- `doc/spec.md` §6.8 (HNSW layout), §6.9 (BM25 layout).
- `crates/nest-runtime/src/ann/` (5-module split).
- `crates/nest-runtime/src/bm25/` (5-module split).
- ADR 0001 (frozen six required sections — supplemented by this ADR).
- ADR 0008 (model_hash gates the search-text path that uses these).
