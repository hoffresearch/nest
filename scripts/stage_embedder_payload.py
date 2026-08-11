"""stage the offline embedder payload for release archives and installers.

the `nest` binary embeds queries OFFLINE by shelling out to the potion
embedder script (`forge/embed_query_potion.py`) with its vendored table.
a released binary has no repo around it, so the release archives and the
one-liner installer carry this payload and lay it down where the cli looks
(`<exe>/../share/nest/forge/` or `$XDG_DATA_HOME/nest/forge/`; see
crates/nest-cli/src/cmd/util.rs `default_potion_embedder_path`).

usage:  python scripts/stage_embedder_payload.py <dest>
writes: <dest>/nest/forge/__init__.py
        <dest>/nest/forge/embed_default.py
        <dest>/nest/forge/embed_potion.py
        <dest>/nest/forge/embed_query_potion.py
        <dest>/nest/forge/models/potion-base-8M/...

git-lfs pointer files are rejected; run `git lfs pull` first. `nest doctor`
validates exactly this layout post-install.
"""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORGE = ROOT / "python" / "forge"

MODULES = [
    "__init__.py",
    "embed_default.py",
    "embed_potion.py",
    "embed_query_potion.py",
]


def fail(msg: str) -> None:
    print(f"stage_embedder_payload: error: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: stage_embedder_payload.py <dest>")
    dest = Path(sys.argv[1]).resolve() / "nest" / "forge"
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for name in MODULES:
        src = FORGE / name
        if not src.is_file():
            fail(f"missing source: {src}")
        shutil.copyfile(src, dest / name)
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


if __name__ == "__main__":
    main()
