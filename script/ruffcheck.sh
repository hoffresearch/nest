#!/bin/sh
# script/ruffcheck.sh -- ruff lint + format check on the python files we own.
#
# ONE list, used by script/fullcheck.sh (best-effort, skipped when ruff
# is not importable) and by .github/workflows/gatecheck.yml (mandatory). files not
# on the list are legacy / vendored / generated and are tracked separately;
# when you touch a python module, add it here and make it clean.
#
#   URNA_PYTHON=.venv/bin/python sh script/ruffcheck.sh
set -eu
cd "$(dirname "$0")/.."
PY="${URNA_PYTHON:-python3}"
TARGETS="
python/embed_query.py
python/model_fingerprint.py
python/builder.py
python/tools/measure_presets.py
python/tools/_bench_runner.py
python/tools/compare_measure.py
python/forge/embed_image.py
python/forge/test_open_clip_snapshot.py
python/forge/image_items.py
python/forge/image_media.py
python/forge/image_encode.py
python/forge/image_encode_still.py
python/forge/image_gop_probe.py
python/forge/image_decode.py
python/forge/image_backends.py
python/forge/image_backends_av1.py
python/forge/image_order.py
python/forge/image_corpus.py
python/tools/urna_build_image_corpus.py
demos/quickstart/quickstart.py
python/tools/urna_search_image.py
python/tools/urna_image_eval.py
python/tools/_image_metrics.py
python/tools/urna_image_sweep.py
tests/test_search_text_model_hash.py
tests/test_image_corpus.py
tests/test_blob_bridge.py
tests/test_space_bridge.py
python/forge/model_registry.py
python/forge/embed_st.py
python/forge/build_spec.py
python/forge/spec_paths.py
python/forge/corpus_sources.py
python/forge/forge_pipeline.py
python/forge/forge_cache.py
python/forge/forge_recipe.py
python/forge/forge_media_stage.py
python/forge/forge_manifest.py
python/forge/quality_gate.py
python/forge/quality_utility.py
python/forge/media_profiles.py
python/forge/embed_query_model.py
python/forge/model_catalog.py
python/forge/install_model.py
python/tools/urna_forge.py
python/tools/urna_model_bench.py
python/tools/_model_bench_report.py
python/tools/urna_ui_bridge.py
tests/test_forge_spec.py
tests/test_quality_gate.py
tests/test_cli_space.py
tests/test_query_embedder_routing.py
tests/test_embedder_payload.py
tests/test_bench_runner.py
tests/test_model_catalog.py
tests/test_model_install.py
script/stage_embedder_payload.py
script/preflight.py
tests/test_preflight.py
script/pypiindex.py
tests/test_pypiindex.py
script/rehearsal.py
tests/test_rehearsal.py
script/chanprobe.py
tests/test_chanprobe.py
tests/test_releasepr.py
"
# shellcheck disable=SC2086
"$PY" -m ruff check $TARGETS
# shellcheck disable=SC2086
"$PY" -m ruff format --check $TARGETS
