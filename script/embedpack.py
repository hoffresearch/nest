"""stage the offline embedder payload for release archives and installers.

the `urna` binary embeds queries OFFLINE by shelling out to a query
embedder script: `forge/embed_query_potion.py` with its vendored table for
potion corpora, `forge/embed_query_model.py` (the model registry) for
corpora whose default model is a registry model (wemm, clip, jina). a
released binary has no repo around it, so the release archives and the
one-liner installer carry this payload and lay it down where the cli looks
(`<exe>/../share/urna/forge/` or `$XDG_DATA_HOME/urna/forge/`; see
crates/urna-clitui/src/cmd/embed_gate.rs `installed_script_in`).

the registry path ships its scripts only, not its model dependencies: the
setup venv has numpy and tokenizers, and `embed_query_model.py` names the
exact `pip install` line for what a registry model still needs (torch,
sentence-transformers, open_clip), exit 4, instead of the old "embedder
script not found".

usage:  python script/embedpack.py <dest> [--tar <out.tar.gz>]
writes: <dest>/urna/model_fingerprint.py          (imported by the registry)
        <dest>/urna/embed_query.py                (st models outside the registry)
        <dest>/urna/VERSION                       (the workspace version: setup
                                                   replaces a payload of another release)
        <dest>/urna/forge/__init__.py
        <dest>/urna/forge/embed_default.py
        <dest>/urna/forge/embed_potion.py
        <dest>/urna/forge/embed_query_potion.py
        <dest>/urna/forge/embed_query_model.py      (the registry embedder)
        <dest>/urna/forge/model_registry.py
        <dest>/urna/forge/model_adapters.py
        <dest>/urna/forge/embed_st.py
        <dest>/urna/forge/embed_st_worker.py
        <dest>/urna/forge/embed_image.py
        <dest>/urna/forge/catalog.json              (the installable models, from the registry)
        <dest>/urna/forge/install_model.py          (fetches one into the hf cache, verified)
        <dest>/urna/forge/models/potion-base-8M/...

with --tar, also packs the staged `urna/` tree as a single gzipped tarball
(the release artifact the one-liner installer downloads and extracts into
the data dir). git-lfs pointer files are rejected; run `git lfs pull`
first. `urna doctor` validates exactly this layout post-install.
"""

from __future__ import annotations

import re
import shutil
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORGE = ROOT / "python" / "forge"

MODULES = [
    "__init__.py",
    "embed_default.py",
    "embed_potion.py",
    "embed_query_potion.py",
    # the registry query path (`ask`/`retrieve` on a wemm, clip or jina
    # corpus): the embedder, the registry, its adapters and the two model
    # backends the adapters import lazily.
    "embed_query_model.py",
    "model_registry.py",
    "model_adapters.py",
    "embed_st.py",
    "embed_st_worker.py",
    "embed_image.py",
    # the models setup and the explorer offer to install (generated from the
    # registry by model_catalog.py; a stale copy fails the stage).
    "catalog.json",
    # the fetch half of the model install setup and the explorer share.
    "install_model.py",
]

# `python/` modules the registry imports as top-level names (the scripts put
# their grandparent dir, `<dest>/urna/`, on sys.path).
TOP_LEVEL_MODULES = [
    "model_fingerprint.py",
    # the sentence-transformers query embedder: `search-text`'s default and
    # the fallback of `embed_query_model.py` for a model no preset names.
    "embed_query.py",
]


def workspace_version(manifest: Path | None = None) -> str:
    """The version every crate and the wheel ship under; at release time it is
    the tag's, so the stamp names the release the payload came from.

    dist's global release job runs this with the runner's python3, which is
    3.10 on ubuntu-22.04 and has no tomllib (3.11+), so without it the
    `version` line of [workspace.package] is read directly."""
    text = (manifest or ROOT / "Cargo.toml").read_text(encoding="utf-8")
    try:
        import tomllib
    except ModuleNotFoundError:
        section = re.search(r"^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
        version = section and re.search(r'^version\s*=\s*"([^"]+)"\s*$', section.group(1), re.M)
        if not version:
            msg = "embedpack: no version in [workspace.package] of Cargo.toml"
            raise SystemExit(msg) from None
        return version.group(1)
    return tomllib.loads(text)["workspace"]["package"]["version"]


def catalog_drift() -> str | None:
    """An error when python/forge/catalog.json is not what the registry
    generates now: the payload must offer exactly the validated presets."""
    sys.path.insert(0, str(ROOT / "python"))
    from forge import model_catalog

    want = model_catalog.render(model_catalog.build())
    have = (FORGE / "catalog.json").read_text() if (FORGE / "catalog.json").is_file() else ""
    if have != want:
        return (
            "python/forge/catalog.json is stale: run python python/forge/model_catalog.py --write"
        )
    return None


def fail(msg: str) -> None:
    print(f"embedpack: error: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    args = sys.argv[1:]
    tar_out: Path | None = None
    if "--tar" in args:
        i = args.index("--tar")
        tar_out = Path(args[i + 1]).resolve()
        del args[i : i + 2]
    if len(args) != 1:
        fail("usage: embedpack.py <dest> [--tar <out.tar.gz>]")
    sources = [(FORGE / n, Path(n)) for n in MODULES]
    sources += [(ROOT / "python" / n, Path("..") / n) for n in TOP_LEVEL_MODULES]
    for src, _ in sources:
        if not src.is_file():
            fail(f"missing source: {src}")
    stale = catalog_drift()
    if stale:
        fail(stale)
    dest = Path(args[0]).resolve() / "urna" / "forge"
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for src, rel in sources:
        shutil.copyfile(src, (dest / rel).resolve())
    (dest.parent / "VERSION").write_text(workspace_version() + "\n")
    model_src = FORGE / "models" / "potion-base-8M"
    if not model_src.is_dir():
        fail(f"missing model dir: {model_src}")
    shutil.copytree(model_src, dest / "models" / "potion-base-8M")
    for f in sorted(dest.rglob("*")):
        if f.is_file():
            with f.open("rb") as fh:
                if fh.read(64).startswith(b"version https://git-lfs"):
                    fail(f"{f} is a git-lfs pointer; run `git lfs pull` first")
    total = sum(f.stat().st_size for f in dest.rglob("*") if f.is_file())
    print(f"staged embedder payload at {dest} ({total / 1e6:.1f} MB)")
    if tar_out is not None:
        tar_out.parent.mkdir(parents=True, exist_ok=True)
        with tarfile.open(tar_out, "w:gz") as tar:
            tar.add(dest.parent, arcname="urna")
        # the one-liner installer verifies this against the downloaded file,
        # in the same `<hex> *<name>` format sha256sum emits.
        import hashlib

        digest = hashlib.sha256(tar_out.read_bytes()).hexdigest()
        sha_path = tar_out.with_name(tar_out.name + ".sha256")
        sha_path.write_text(f"{digest} *{tar_out.name}\n")
        print(f"wrote {tar_out} ({tar_out.stat().st_size / 1e6:.1f} MB)")
        print(f"wrote {sha_path}")


if __name__ == "__main__":
    main()
