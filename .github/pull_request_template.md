## what and why

<!-- what changes, why, and how it was tested. this text becomes the squash commit on main. -->

## afterwork

tick what the change touched; leave the rest. the full walk is `.contracts/.agents/.skills/afterwork/AFTERWORK.md`, over the file list in `.contracts/.agents/.skills/afterwork/specs.yaml`.

- [ ] `docs/CHANGELOG` `[Unreleased]`: the why, measured numbers, test counts
- [ ] `docs/ARC.toml`: architecture, contracts, inventory, dated `summary` note
- [ ] `.contracts/.agents/AGENTS.md`: commands, gotchas, known gaps, layout
- [ ] tests: happy path, error path, edge case, real artifacts
- [ ] user-visible: `README.md`, `llms.txt`, `docs/USAGE.md`, `examples/`, `assets/images/`
- [ ] format or decoders: roundtrip + `negative_*.rs`, fuzz arm, baselines, `docs/BENCH.md`
- [ ] python, packaging, release: `scripts/ruff_check.sh`, pyproject files, dist config, `install-test.yml`
- [ ] security and data: `docs/SECURITY.md`, `scripts/pre-commit`, `.gitattributes`

## gate

- [ ] `scripts/release_check.sh` passes (fmt, clippy `-D warnings`, tests, ruff)
- [ ] `forge-core` tested on its own manifest, if touched
- [ ] file hygiene: files over the line limit split by responsibility, nothing dead, stray or misplaced left
- [ ] no em dash or emoji
