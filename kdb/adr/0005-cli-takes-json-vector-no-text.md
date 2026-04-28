# ADR 0005: `nest search` accepts a JSON-array vector, not text

- **Status:** Superseded by ADR 0010 (2026-04-28)
- **Date:** 2026-04-27
- **Deciders:** project owner

## Context

`nest search file.nest "vacina covid" --k 5` is the natural ergonomic
for a semantic-search CLI. Implementing it requires bundling an
embedding model into the `nest` binary so the CLI can convert text →
vector at invocation time.

Embedding models are 80–500 MB, hold heavy ML runtime dependencies
(`tokenizers`, `safetensors`, ONNX, candle, …), and pin the binary to
a single `embedding_model` value baked at compile time.

## Decision

`nest search` accepts the query as a **JSON array of f32**. The binary
ships no embedding model. Text → vector is the caller's responsibility:

- Python users: `nest.open(p).search(qvec, k)` after embedding via
  whatever model they prefer.
- Shell users: pre-compute the JSON-array externally (e.g. via
  `python -c "..."`) and pass it as the second positional argument.

The CLI's `--help` and `doc/spec.md` §17 both document this.

## Consequences

### Positive

- `nest-cli` stays a single ~3 MB stripped binary with zero ML
  dependencies. It can ship to constrained runtimes.
- The format remains agnostic about the embedding model used to
  produce the vectors. The model identity is recorded only in the
  manifest's `embedding_model` and `model_hash` fields.

### Negative

- The literal command `nest search file.nest "vacina covid" --k 5`
  fails with `Error: invalid query JSON`. New users hit this once
  before reading the help text.

### Trade-offs

- Did **not** add a `--text` flag in v0.1.0 because there is no
  consensus model to bundle. A future `nest-cli-text` companion crate
  could ship with an opinionated default model.

## Alternatives considered

- **Bundle a default model in the binary.** Rejected for v0.1.0:
  freezes the binary to one model, bloats the binary by an order of
  magnitude, complicates the build matrix.
- **Accept either text or JSON, auto-detect.** Rejected: even if the
  binary had a model, "auto-detect" hides which path produced the
  vectors and makes test reproduction harder.

## References

- `crates/nest-cli/src/main.rs::cmd_search`
- `doc/spec.md` §17 (CLI surface)
- `README.md` "Text queries" section
