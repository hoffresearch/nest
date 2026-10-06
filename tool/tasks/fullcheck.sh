#!/usr/bin/env bash
# fullcheck.sh - full release verification pipeline.
#
# Runs the local release gate end-to-end (what it skips: docs/USAGE.md section 10):
#   0. the name check (length table, exceptions, lexicon)
#   1. cargo build (release), then the PyO3 extension (.so) the tests load
#   2. cargo test/clippy/fmt (release profile), the 639-line guard
#   3. the python suites (the `step "python tests/..."` lines below), ruff
#   4. presetrun --json on the LFS-tracked corpus
#   5. benchgate regression gates vs data/measure/baseline.json
#
# Exits non-zero on the first failure. Total runtime is 45 to 60 min on an
# m-series mac with a warm cargo cache: steps 1 to 3 take about 3 min, and
# step 4 rebuilds every preset and mrl variant of the 30,725-chunk corpus
# from scratch (twelve builds of 4 to 5 min each).
#
# Override knobs (env vars):
#   URNA_BASELINE  - baseline JSON to compare against (default: data/measure/baseline.json)
#   URNA_QUERIES   - presetrun query count (default: 100)
#   URNA_K         - presetrun top-k (default: 10)
#   URNA_PYTHON    - python interpreter (default: ./.venv/bin/python if present, else python3)
#   URNA_OUT       - where to write the post-run JSON (default: /tmp/fullcheck_post.json)

set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT="$(pwd)"

# ---- knobs ----
BASELINE="${URNA_BASELINE:-data/measure/baseline.json}"
QUERIES="${URNA_QUERIES:-100}"
K="${URNA_K:-10}"
OUT="${URNA_OUT:-/tmp/fullcheck_post.json}"

if [[ -n "${URNA_PYTHON:-}" ]]; then
  PY="$URNA_PYTHON"
elif [[ -x "$ROOT/.venv/bin/python" ]]; then
  PY="$ROOT/.venv/bin/python"
else
  PY="$(command -v python3)"
fi

step() {
  printf '\n\033[1;36m== %s ==\033[0m\n' "$*" >&2
}

ok() {
  printf '\033[1;32m  PASS:\033[0m %s\n' "$*" >&2
}

# ---- names ----
step "python tool/tasks/namecheck.py"
"$PY" tool/tasks/namecheck.py
"$PY" tool/tests/test_namecheck.py
ok "names follow the length table and the lexicon"

# ---- cargo build (release) ----
step "cargo build --release --workspace"
cargo build --release --workspace
ok "release build"

# ---- rebuild PyO3 .so ----
# before cargo test: the cli e2e tests (cli_e2e.rs) build their demo corpus
# through the urna package, so a fresh checkout without
# rust/bridge/python/urna/_urna.so failed
# there before reaching this step. the copy is what the tests load; a later
# cargo build of urna-bridge without the feature does not touch it.
step "rebuild rust/bridge/python/urna/_urna.so"
# build the extension against the SAME interpreter that runs the tests, so a
# .venv that differs from the default build python can never load a mismatched
# _urna.so (that mismatch segfaults test_pythonapi). PYO3_PYTHON pins it to $PY.
# pyo3/extension-module keeps libpython OUT of the dylib (extension modules
# resolve symbols from the host process): without it the .so hard-links a
# libpython path and segfaults under statically-embedded interpreters (uv's
# python-build-standalone) by loading a second runtime. maturin builds the
# published wheel the same way.
PYO3_PYTHON="$PY" cargo build --release -p urna-bridge \
  --features pyo3/extension-module >/dev/null
case "$(uname)" in
  Darwin) cp target/release/lib_urna.dylib rust/bridge/python/urna/_urna.so ;;
  Linux)  cp target/release/lib_urna.so    rust/bridge/python/urna/_urna.so ;;
  *) printf "unknown OS, copy lib_urna.* manually\n" >&2; exit 1 ;;
esac
ok "_urna.so built and copied"

# ---- cargo test (release) ----
step "cargo test --release --workspace"
cargo test --release --workspace 2>&1 \
  | grep -E "^(test result|running [0-9]+ tests)" \
  | awk '/^test result/ { passed += $4; failed += $6; ignored += $8 } END { printf "  passed=%d failed=%d ignored=%d\n", passed, failed, ignored; if (failed > 0) exit 1 }'
ok "all tests"

# ---- cargo clippy ----
step "cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings >/dev/null 2>&1
ok "clippy clean"

# ---- cargo fmt ----
step "cargo fmt --all --check"
cargo fmt --all --check
ok "rustfmt clean"

# ---- 639-line guard ----
step "no Rust file in rust/**/src exceeds 639 lines"
overlong="$(find rust -name '*.rs' -not -path '*/tests/*' \
  | xargs wc -l \
  | awk '$1 > 639 {print}' \
  | grep -v 'total$' || true)"
if [[ -n "$overlong" ]]; then
  printf '\033[1;31m  FAIL:\033[0m\n%s\n' "$overlong" >&2
  exit 1
fi
ok "all source files ≤ 639 lines"

# ---- python tests ----
step "python tool/tests/test_pythonapi.py"
"$PY" tool/tests/test_pythonapi.py
ok "pythonapi"

step "python tool/tests/test_ingestion.py"
"$PY" tool/tests/test_ingestion.py
ok "ingestion"

step "python tool/tests/test_hashguard.py"
"$PY" tool/tests/test_hashguard.py
ok "hashguard: search-text model_hash gate (7 cases)"

# builds its own dataset, so it needs no demo corpus; the compressed cases
# skip themselves when ffmpeg/libsvtav1 is absent.
step "python tool/tests/test_imagepipe.py"
"$PY" tool/tests/test_imagepipe.py
ok "imagepipe: image corpus pipeline (43 cases)"

# declarative builds: spec validation, fake-preset e2e, triad cache, dedup,
# output modes, L3 rebuild. no heavy ML deps; media legs skip without ffmpeg.
step "python tool/tests/test_specrules.py"
"$PY" tool/tests/test_specrules.py
ok "specrules: build spec + pipeline"

# dual quality gate + jxl round-trip; skips cleanly without ssimulacra2/cjxl.
step "python tool/tests/test_mediagate.py"
"$PY" tool/tests/test_mediagate.py
ok "mediagate: quality gate + jxl"

step "python tool/tests/test_clispaces.py"
"$PY" tool/tests/test_clispaces.py
ok "clispaces: cli space verbs (8 cases)"

step "python tool/tests/test_askrouter.py"
"$PY" tool/tests/test_askrouter.py
ok "askrouter: query embedder routing (10 cases; 7 to 10 need sentence-transformers, URNA_ST_PYTHON)"

# the release payload, staged and run from outside the checkout: both query
# embedders answer, the registry route names its missing deps, a half tree
# does not stage.
step "python tool/tests/test_embedpack.py"
"$PY" tool/tests/test_embedpack.py
ok "embedpack: embedder payload (3 cases)"

# the model catalog setup offers is the registry's validated presets, with a
# reason for every preset it leaves out.
step "python tool/tests/test_catalogue.py"
"$PY" tool/tests/test_catalogue.py
ok "catalogue: model catalog (5 cases)"

# the model fetch: confirmed downloads only, the pinned files only, kept
# only when the fingerprint is the catalog's (an 18 MB hub model; the
# download cases skip by name when huggingface.co does not answer).
step "python tool/tests/test_modelpull.py"
"$PY" tool/tests/test_modelpull.py
ok "modelpull: model install (5 cases)"

# the benchmark rebuild builds beside the corpus and renames at the end, so
# an interrupted gate never leaves data/measure without its corpora.
step "python tool/tests/test_benchmark.py"
"$PY" tool/tests/test_benchmark.py
ok "benchmark: bench runner (4 cases)"

# the version a release names agrees across the manifests, the lockfile,
# CITATION.cff and the changelog; the tag checks need a tag and run in ci.
step "python tool/tests/test_preflight.py"
"$PY" tool/tests/test_preflight.py
ok "preflight (7 cases)"

# the pypi upload takes the release's wheels from the run its index allows,
# and a rerun uploads only what the index does not have yet.
step "python tool/tests/test_pypiindex.py"
"$PY" tool/tests/test_pypiindex.py
ok "pypiindex (5 cases)"

# the release rehearsal is generated from release.yml, publishes nothing,
# and its required check fails a needed build that did not pass.
step "python tool/tests/test_rehearsal.py"
"$PY" tool/tests/test_rehearsal.py
ok "rehearsal (7 cases)"

# setuptest waits for the exact version on each registry, and the release
# report names what every channel serves, a failed or cancelled run included.
step "python tool/tests/test_chanprobe.py"
"$PY" tool/tests/test_chanprobe.py
ok "chanprobe (8 cases)"

# the release pull request is prepared by cargo-release in its own worktree,
# signed, and touches only the version, the lockfile, the changelog and
# CITATION.cff; skips without the pinned cargo-release.
step "python tool/tests/test_releasepr.py"
"$PY" tool/tests/test_releasepr.py
ok "releasepr (4 cases)"

# ---- ruff (best-effort) ----
# the file list lives in tool/tasks/ruffcheck.sh so gatecheck.yml and this gate stay
# in lockstep; ruff missing from $PY is a skip here, a failure in ci.
if "$PY" -c "import ruff" 2>/dev/null || "$PY" -m ruff --version 2>/dev/null | head -1 >/dev/null; then
  step "ruff check / format on the files we own (tool/tasks/ruffcheck.sh)"
  URNA_PYTHON="$PY" sh tool/tasks/ruffcheck.sh
  ok "ruff clean"
else
  printf '  skip: ruff not importable in %s\n' "$PY" >&2
fi

# ---- presetrun + compare ----
step "python tool/bench/presetrun.py --n-queries $QUERIES --k $K --json"
"$PY" tool/bench/presetrun.py --n-queries "$QUERIES" --k "$K" --json > "$OUT"
ok "metrics written to $OUT"

step "python tool/bench/benchgate.py $BASELINE $OUT"
"$PY" tool/bench/benchgate.py "$BASELINE" "$OUT"
ok "regression gates"

# ---- summary ----
printf '\n\033[1;32m== fullcheck passed ==\033[0m\n'
printf '  baseline: %s\n' "$BASELINE"
printf '  post:     %s\n' "$OUT"
printf '  next:     tool/tasks/releasepr.sh X.Y.Z (docs/USAGE.md, maintainer checklist step 8)\n'
