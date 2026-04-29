<!-- title in plain english, no Conventional Commits prefix. body explains the why; the diff already shows the what. -->

## summary

<!-- 1-3 bullets or one paragraph: what this PR changes and why. -->

## type

- [ ] bug fix
- [ ] new feature
- [ ] refactor / cleanup
- [ ] docs only
- [ ] format change (touches binary layout, hash semantics, manifest, or runtime contract)
- [ ] other: ...

## checks

- [ ] `cargo test --release --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all --check` clean
- [ ] python tests pass: `tests/test_e2e.py`, `tests/test_builder.py`, `tests/test_search_text_model_hash.py`
- [ ] `ruff check . && ruff format --check .` clean
- [ ] `./scripts/release_check.sh` exits 0 (the canonical gate)

## format / runtime impact

<!-- if this PR touches the binary layout, the search contract, hash semantics, or the model fingerprint, write or update an ADR under kdb/adr/. otherwise: n/a. -->

n/a

## breaking changes

<!-- list user-visible behavior changes, or write n/a. -->

n/a

## related

<!-- linked issues, ADRs, prior PRs. -->
