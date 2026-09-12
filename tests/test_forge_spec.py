"""Prove the declarative build contract (RFC-1) with the fake preset:
spec validation errors name their key; total ordering is enforced; the
fake e2e build emits valid multi-space files in all three output modes
with dedup'd media, shared blob spans and citation-consistent chunk_ids;
the N2 triad invalidates caches on any content change; --rebuild-only is
byte-identical (L3); a corrupted cache is recomputed, never reused;
${VAR} in spec paths expands strictly (unset names the key).

Run: .venv/bin/python tests/test_forge_spec.py
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))
os.environ["NEST_ENABLE_FAKE_PRESET"] = "1"

import nest
import numpy as np
from forge.build_spec import SpecError, load_spec, validate
from forge.forge_pipeline import build

HAVE_FFMPEG = shutil.which("ffmpeg") is not None


def _fixture(
    base: Path, with_media: bool = True, mode: str = "both", embed_media: bool = False
) -> Path:
    from PIL import Image

    rng = np.random.default_rng(1)
    rows = []
    for i in range(12):
        img = base / f"img{i:02d}.png"
        arr = rng.integers(0, 255, (48 + i, 64, 3), dtype=np.uint8)
        if i in (5, 9):  # byte-identical duplicates of img01 -> dedup
            arr = np.array(Image.open(base / "img01.png"))
        Image.fromarray(arr).save(img)
        rows.append(
            {
                "id": f"k{i:02d}",
                "title": f"Item {i}",
                "body": f"body text {i}" if i % 3 else "",
                "img": str(img.resolve()),
            }
        )
    (base / "rows.jsonl").write_text("\n".join(json.dumps(r) for r in rows))
    media = (
        """
[media]
backend = "av1"
crf = 40
speed = 10
shard_size = 8
"""
        if with_media
        else ""
    )
    image_block = (
        """
[source.image]
path_template = "{img}"
label_template = "{title}"
"""
        if with_media
        else ""
    )
    fake_image = 'image = "space"' if with_media else ""
    spec = base / "spec.toml"
    spec.write_text(f"""
[corpus]
name = "faketest"
chunker_version = "fake/1"

[source]
kind = "jsonl"
path = "{(base / "rows.jsonl").resolve()}"
order_by = ["id"]
[source.text]
template = \"\"\"
{{title}}
{{body}}
\"\"\"
{image_block}
{media}
[[models]]
preset = "potion"
text = "default"

[[models]]
preset = "fake-test"
text = "space"
{fake_image}
dims = [4]
space_dtype = "float32"

[build]
preset = "hybrid"
dtype = "int8"

[output]
mode = "{mode}"
dir = "{base / "out"}"
{"embed_media = true" if embed_media else ""}
""")
    return spec


def _expect_spec_error(toml_text: str, needle: str, base: Path) -> None:
    p = base / "bad.toml"
    p.write_text(toml_text)
    try:
        validate(load_spec(p))
    except SpecError as e:
        assert needle in str(e), f"expected '{needle}' in: {e}"
    else:
        raise AssertionError(f"expected SpecError containing '{needle}'")


MINIMAL = """
[corpus]
name = "x"
chunker_version = "v"
[source]
kind = "jsonl"
path = "/tmp/none.jsonl"
order_by = ["id"]
{models}
[output]
dir = "/tmp/o"
{extra}
"""


def test_validation_errors(base: Path) -> None:
    _expect_spec_error(MINIMAL.format(models="", extra=""), "at least one", base)
    two = (
        '[[models]]\npreset="potion"\ntext="default"\n'
        '[[models]]\npreset="fake-test"\ntext="default"\n'
    )
    _expect_spec_error(MINIMAL.format(models=two, extra=""), "exactly one", base)
    bad_dims = '[[models]]\npreset="fake-test"\ntext="default"\ndims=[3]\n'
    _expect_spec_error(MINIMAL.format(models=bad_dims, extra=""), "validated ladder", base)
    heavy = '[[models]]\npreset="wemm-4b"\ntext="default"\n'
    _expect_spec_error(MINIMAL.format(models=heavy, extra="[output.x]"), "unknown key", base)
    _expect_spec_error(MINIMAL.format(models=heavy, extra=""), "allow-heavy", base)
    remote = '[[models]]\npreset="wemm-2b"\ntext="default"\n'
    _expect_spec_error(MINIMAL.format(models=remote, extra=""), "allow_remote_code", base)
    print("test_validation_errors: OK")


def test_media_profiles(base: Path) -> None:
    """media.profile resolves measured knob defaults; explicit keys win."""
    from forge.build_spec import MEDIA_PROFILES

    models = '[[models]]\npreset="potion"\ntext="default"\n'

    def parse(extra: str):
        p = base / "prof.toml"
        p.write_text(MINIMAL.format(models=models, extra=extra))
        return load_spec(p)

    m = parse('[media]\nprofile = "near-dup"').media
    assert (m.profile, m.order, m.gop, m.tune) == ("near-dup", "cluster", "auto", "still")
    m = parse('[media]\nprofile = "near-dup"\norder = "none"').media
    assert m.order == "none" and m.tune == "still", "explicit key must win over the profile"
    m = parse('[media]\nprofile = "archive"').media
    assert m.backend == "jxl-transcode"
    m = parse('[media]\nprofile = "stills"').media
    assert (m.gop, m.tune) == ("intra", "still")
    m = parse('[media]\nprofile = "retrieval"').media
    assert (m.gop, m.tune, m.speed, m.crf) == ("intra", "still", 6, 50)
    assert m.backend == "av1" and m.quality.drift_floor_p10 == 0.98, "gate defaults untouched"
    m = parse('[media]\nprofile = "retrieval"\ncrf = 45').media
    assert m.crf == 45 and m.speed == 6, "explicit crf wins, the rest of the profile stays"
    m = parse("[media]").media
    assert m.profile == "" and m.gop == "auto", "no profile keeps the schema defaults"
    assert set(MEDIA_PROFILES) == {"near-dup", "stills", "archive", "retrieval"}
    _expect_spec_error(
        MINIMAL.format(models=models, extra='[media]\nprofile = "cards"'),
        "media.profile",
        base,
    )
    print("test_media_profiles: OK")


def test_env_expansion(base: Path) -> None:
    prior = os.environ.get("NEST_FORGE_TEST_DATA")
    try:
        _env_expansion_body(base)
    finally:
        if prior is None:
            os.environ.pop("NEST_FORGE_TEST_DATA", None)
        else:
            os.environ["NEST_FORGE_TEST_DATA"] = prior
    print("test_env_expansion: OK")


def _env_expansion_body(base: Path) -> None:
    d = base / "env"
    (d / "data").mkdir(parents=True)
    rows = [{"id": f"r{i}", "title": f"Row {i}"} for i in range(3)]
    (d / "data" / "rows.jsonl").write_text("\n".join(json.dumps(r) for r in rows))
    spec_p = d / "env.toml"
    spec_p.write_text(f"""
[corpus]
name = "envtest"
chunker_version = "v"
[source]
kind = "jsonl"
path = "${{NEST_FORGE_TEST_DATA}}/rows.jsonl"
order_by = ["id"]
[source.text]
template = "{{title}} costs $5"
[[models]]
preset = "potion"
text = "default"
[output]
dir = "{d / "out"}"
""")
    os.environ["NEST_FORGE_TEST_DATA"] = str(d / "data")
    spec = load_spec(spec_p)
    validate(spec)
    assert spec.source.path == str(d / "data" / "rows.jsonl"), "braced var must expand"
    assert spec.source.text.template == "{title} costs $5", (
        "bare $5 and {col} placeholders must survive"
    )
    result = build(spec)
    assert result["n_items"] == 3
    nest.open(result["outputs"]["envtest.nest"]["file"]).validate()
    # the lock stores the expanded path; a rebuild under a moved data root
    # (same rows, another location) must still claim L3 under --strict-env
    shutil.copytree(d / "data", d / "data-b")
    os.environ["NEST_FORGE_TEST_DATA"] = str(d / "data-b")
    again = build(load_spec(spec_p), rebuild_only=True, strict_env=True)
    assert (
        again["outputs"]["envtest.nest"]["file_hash"]
        == (result["outputs"]["envtest.nest"]["file_hash"])
    ), "rebuild-only under another data root must stay byte-identical"
    for state, setup in (("is not set", None), ("is empty", "")):
        if setup is None:
            os.environ.pop("NEST_FORGE_TEST_DATA")
        else:
            os.environ["NEST_FORGE_TEST_DATA"] = setup
        try:
            load_spec(spec_p)
        except SpecError as e:
            msg = str(e)
            assert "source.path" in msg and "NEST_FORGE_TEST_DATA" in msg, msg
            assert state in msg and "export" in msg, msg
        else:
            raise AssertionError(f"a ${{VAR}} that {state} must be a SpecError, never a silent '$'")
    # no expanduser mid-string, bare $ untouched, and ~/ still expands
    from forge.spec_paths import expand_paths

    os.environ["NEST_FORGE_TEST_DATA"] = "/x"
    out = expand_paths(
        {"source": {"db": "${NEST_FORGE_TEST_DATA}/~x", "query": "cost > $5"}},
    )
    assert out == {"source": {"db": "/x/~x", "query": "cost > $5"}}, out
    assert expand_paths({"text": {"template": "{t} $5 ${NEST_FORGE_TEST_DATA}"}}) == {
        "text": {"template": "{t} $5 /x"}
    }, "the braced var expands inside a template while {col} and $5 survive"
    assert expand_paths(["~/x"]) == [os.path.expanduser("~/x")]
    os.environ["NEST_FORGE_TEST_DATA"] = "~/via-var"
    assert expand_paths("${NEST_FORGE_TEST_DATA}/y") == os.path.expanduser("~/via-var/y"), (
        "a variable holding ~/ expands too"
    )
    try:
        expand_paths({"models": [{"preset": "p"}, {"model_path": "${NEST_FORGE_UNSET_X}"}]})
    except SpecError as e:
        assert str(e).startswith("models[1].model_path:"), str(e)
    else:
        raise AssertionError("dotted key path must reach into lists")
    # the module must import on its own in a fresh interpreter (build_spec
    # imports it at its bottom; SpecError is resolved lazily at raise time)
    probe = subprocess.run(
        [sys.executable, "-c", "from forge.spec_paths import expand_paths"],
        cwd=REPO / "python",
        capture_output=True,
        text=True,
    )
    assert probe.returncode == 0, probe.stderr


def test_total_ordering(base: Path) -> None:
    d = base / "dupes"
    d.mkdir()
    (d / "rows.jsonl").write_text('{"id": "a", "t": "x"}\n{"id": "a", "t": "y"}')
    spec_p = d / "s.toml"
    spec_p.write_text(f"""
[corpus]
name = "d"
chunker_version = "v"
[source]
kind = "jsonl"
path = "{d / "rows.jsonl"}"
order_by = ["id"]
[source.text]
template = "{{t}}"
[[models]]
preset = "potion"
text = "default"
[output]
dir = "{d / "out"}"
""")
    try:
        build(load_spec(spec_p))
    except SpecError as e:
        assert "total order" in str(e)
    else:
        raise AssertionError("duplicate order_by keys must fail")
    print("test_total_ordering: OK")


def test_e2e_fake(base: Path) -> None:
    if not HAVE_FFMPEG:
        print("test_e2e_fake: SKIP (no ffmpeg)")
        return
    d = base / "e2e"
    d.mkdir()
    spec = load_spec(_fixture(d, with_media=True, mode="both"))
    result = build(spec)
    assert result["n_items"] == 12 and result["n_unique_frames"] == 10, "dedup must collapse dupes"
    out = d / "out"
    dbs = {name: nest.open(str(out / name)) for name in result["outputs"]}
    for db in dbs.values():
        db.validate()
    single = dbs["faketest.nest"]
    assert single.space_names == ["fake-test@4", "fake-test-text@4"]
    hits = single.search_space("fake-test@4", [0.5, 0.5, 0.5, 0.5], 3)
    assert len(hits) == 3 and hits[0].score >= hits[2].score
    # citation consistency: same chunk_ids in the single and per-model files
    q = [1.0] + [0.0] * 255
    ids_single = {h.chunk_id for h in single.search(q, k=12)}
    ids_per = {h.chunk_id for h in dbs["faketest-fake-test.nest"].search(q, k=12)}
    assert ids_single == ids_per, "chunk_ids must be identical across output modes"
    # duplicate rows share the same blob span (N chunks -> 1 frame)
    manifest = json.loads((out / "faketest.manifest.json").read_text())
    assert manifest["manifest_schema_version"] == 1
    uris = [it["media_uri"] for it in manifest["items"]]
    assert uris[1] == uris[5] == uris[9], "dup rows must map to one frame"
    assert manifest["media"]["dedup"]["n_unique_frames"] == 10
    # L3: rebuild from caches is byte-identical
    h1 = (out / "faketest.nest").read_bytes()
    build(load_spec(d / "spec.toml"), rebuild_only=True)
    assert (out / "faketest.nest").read_bytes() == h1, "rebuild-only must be byte-identical"
    print("test_e2e_fake: OK")


def test_embed_media(base: Path) -> None:
    if not HAVE_FFMPEG:
        print("test_embed_media: SKIP (no ffmpeg)")
        return
    d = base / "embed"
    d.mkdir()
    spec = load_spec(_fixture(d, with_media=True, mode="single", embed_media=True))
    build(spec)
    out = d / "out"
    db = nest.open(str(out / "faketest.nest"))
    db.validate()
    refs = db.blob_refs()
    assert refs and all(r["inlined"] for r in refs), "embed_media must inline every blob"
    media_bytes = sum(p.stat().st_size for p in (out / "faketest.media").glob("*") if p.is_file())
    nest_bytes = (out / "faketest.nest").stat().st_size
    assert nest_bytes > media_bytes, "the self-contained file must carry the media bytes"
    manifest = json.loads((out / "faketest.manifest.json").read_text())
    assert manifest["media"]["embedded"] is True
    # the sidecar-mode twin of the same corpus keeps blobs out-of-line
    d2 = base / "embed-side"
    d2.mkdir()
    build(load_spec(_fixture(d2, with_media=True, mode="single")))
    side = nest.open(str(d2 / "out" / "faketest.nest"))
    assert all(not r["inlined"] for r in side.blob_refs())
    print("test_embed_media: OK")


def test_triad_invalidation(base: Path) -> None:
    d = base / "triad"
    d.mkdir()
    spec_p = _fixture(d, with_media=False, mode="single")
    r1 = build(load_spec(spec_p))
    rows = (d / "rows.jsonl").read_text().replace("body text 1", "body text 1 EDITED")
    (d / "rows.jsonl").write_text(rows)
    try:
        build(load_spec(spec_p), rebuild_only=True)
    except Exception as e:
        assert "triad" in str(e) or "stale" in str(e) or "missing" in str(e)
    else:
        raise AssertionError("content change must invalidate the cache under --rebuild-only")
    r2 = build(load_spec(spec_p))
    assert r1["corpus_input_hash"] != r2["corpus_input_hash"]
    print("test_triad_invalidation: OK")


def test_corrupt_cache_recomputed(base: Path) -> None:
    d = base / "corrupt"
    d.mkdir()
    spec_p = _fixture(d, with_media=False, mode="single")
    build(load_spec(spec_p))
    cache = next((d / "out" / ".cache").glob("faketest.potion.npz"))
    cache.write_bytes(cache.read_bytes()[:-7])  # torn write
    result = build(load_spec(spec_p))  # must recompute, not crash or reuse
    nest.open(result["outputs"]["faketest.nest"]["file"]).validate()
    print("test_corrupt_cache_recomputed: OK")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="nest-forge-spec-") as tmp:
        base = Path(tmp)
        test_validation_errors(base)
        test_media_profiles(base)
        test_env_expansion(base)
        test_total_ordering(base)
        test_e2e_fake(base)
        test_embed_media(base)
        test_triad_invalidation(base)
        test_corrupt_cache_recomputed(base)
    print("all forge spec tests passed")


if __name__ == "__main__":
    main()
