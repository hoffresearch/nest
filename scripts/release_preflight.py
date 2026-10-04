"""Check that a release's version and origin agree before anything is built.

Tree mode (every pull request, ``ci.yml``): the workspace version in
``Cargo.toml`` is the one version everywhere a release names it:

- the ``urna-format`` and ``urna-runtime`` pins in ``[workspace.dependencies]``
  (``cargo publish`` keeps the version and strips the path);
- every crate under ``crates/`` inherits it (``version.workspace = true``);
- ``Cargo.lock`` carries it for urna, urna-format, urna-runtime, urna-python;
- ``CITATION.cff``: ``version``, the versioned ``repository-artifact`` and
  release URLs, and a ``date-released`` equal to the changelog's date;
- ``docs/CHANGELOG`` has a ``## [X.Y.Z] - YYYY-MM-DD`` section.

Tag mode (``--tag vX.Y.Z``, the release's ``tag-verify.yml`` after the
signature check): the tree checks, plus the tag names that version, its
commit is on the protected ``main`` and the release date is not after the
day the tag was made (in the tagger's own timezone).

Needs Python 3.11+ (tomllib); the jobs that run it are on ubuntu-24.04.

    python scripts/release_preflight.py
    python scripts/release_preflight.py --tag v0.5.3 --main-ref origin/main
"""

from __future__ import annotations

import argparse
import datetime as dt
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINNED = ("urna-format", "urna-runtime")
LOCKED = ("urna", "urna-format", "urna-runtime", "urna-python")
CRATES_URL = "https://crates.io/crates/urna/{v}"
RELEASE_URL = "https://github.com/hoffresearch/urna/releases/tag/v{v}"
RELEASE_DESCRIPTION = "Release of Urna version {v}."


def _toml(path: Path) -> dict:
    try:
        import tomllib
    except ModuleNotFoundError:
        raise SystemExit("release-preflight: needs Python 3.11+ (tomllib)") from None

    return tomllib.loads(path.read_text(encoding="utf-8"))


def _cff_field(text: str, key: str) -> str | None:
    # top-level and list-item scalars, quoted or bare; CITATION.cff keeps
    # one value per line, so a line match is exact enough without a yaml dep.
    match = re.search(rf'^\s*(?:-\s+)?{re.escape(key)}:\s*"?([^"\n]*?)"?\s*$', text, re.M)
    return match.group(1) if match else None


def _iso_date(value: str | None) -> dt.date | None:
    try:
        return dt.date.fromisoformat(value or "")
    except ValueError:
        return None


def workspace_version(root: Path) -> str:
    return _toml(root / "Cargo.toml")["workspace"]["package"]["version"]


def release_date(root: Path, version: str) -> dt.date | None:
    """The date of the changelog section for `version`, or None."""
    text = (root / "docs" / "CHANGELOG").read_text(encoding="utf-8")
    heading = rf"^## \[{re.escape(version)}\] - (\d{{4}}-\d{{2}}-\d{{2}})\s*$"
    match = re.search(heading, text, re.M)
    return _iso_date(match.group(1)) if match else None


def _check_manifests(root: Path, v: str) -> list[str]:
    errors = []
    deps = _toml(root / "Cargo.toml")["workspace"].get("dependencies", {})
    for name in PINNED:
        pin = deps.get(name, {}).get("version") if isinstance(deps.get(name), dict) else None
        if pin != v:
            errors.append(f"Cargo.toml: [workspace.dependencies] {name} pins {pin!r}, not {v!r}")
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        version = _toml(manifest)["package"].get("version")
        if version != {"workspace": True} and version != v:
            rel = manifest.relative_to(root)
            errors.append(f"{rel}: version {version!r} neither inherits nor equals {v!r}")
    locked = {p["name"]: p["version"] for p in _toml(root / "Cargo.lock").get("package", [])}
    for name in LOCKED:
        if locked.get(name) != v:
            errors.append(f"Cargo.lock: {name} is {locked.get(name)!r}, not {v!r}")
    return errors


def _check_citation(root: Path, v: str, released: dt.date | None) -> list[str]:
    text = (root / "CITATION.cff").read_text(encoding="utf-8")
    want = {
        "version": v,
        "repository-artifact": CRATES_URL.format(v=v),
        "value": RELEASE_URL.format(v=v),
        "description": RELEASE_DESCRIPTION.format(v=v),
    }
    errors = [
        f"CITATION.cff: {key} is {_cff_field(text, key)!r}, not {value!r}"
        for key, value in want.items()
        if _cff_field(text, key) != value
    ]
    cited = _iso_date(_cff_field(text, "date-released"))
    if cited is None:
        errors.append("CITATION.cff: date-released is missing or not YYYY-MM-DD")
    elif released is not None and cited != released:
        msg = f"date-released {cited} differs from the changelog's {released}"
        errors.append(f"CITATION.cff: {msg}")
    return errors


def check_tree(root: Path = ROOT) -> list[str]:
    """Every place a release names its version agrees; returns the errors."""
    v = workspace_version(root)
    released = release_date(root, v)
    errors = _check_manifests(root, v)
    if released is None:
        errors.append(f"docs/CHANGELOG: no '## [{v}] - YYYY-MM-DD' section")
    return errors + _check_citation(root, v, released)


def _git(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)


def check_tag(tag: str, root: Path = ROOT, main_ref: str = "origin/main") -> list[str]:
    """The tree checks plus the tag's name, its place on main and its date."""
    errors = check_tree(root)
    v = workspace_version(root)
    if tag != f"v{v}":
        errors.append(f"tag {tag} does not name the workspace version v{v}")
    commit = _git(root, "rev-parse", "--verify", "-q", f"refs/tags/{tag}^{{commit}}").stdout.strip()
    if not commit:
        return errors + [f"tag {tag} does not exist"]
    if _git(root, "merge-base", "--is-ancestor", commit, main_ref).returncode != 0:
        errors.append(f"tag {tag} points at {commit[:12]}, which is not on {main_ref}")
    fmt = "--format=%(taggerdate:short)"
    tagged = _iso_date(_git(root, "for-each-ref", fmt, f"refs/tags/{tag}").stdout.strip())
    released = release_date(root, v)
    if tagged is None:
        errors.append(f"tag {tag} has no tagger date (a lightweight tag)")
    elif released is not None and released > tagged:
        errors.append(f"release date {released} is after the tag date {tagged}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--tag", help="also check this release tag (tag mode)")
    parser.add_argument("--main-ref", default="origin/main", help="the protected branch ref")
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    errors = check_tag(args.tag, args.root, args.main_ref) if args.tag else check_tree(args.root)
    for error in errors:
        print(f"release-preflight: {error}", file=sys.stderr)
    if errors:
        return 1
    scope = f"tag {args.tag}" if args.tag else "tree"
    print(f"release-preflight: {scope} ok, version {workspace_version(args.root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
