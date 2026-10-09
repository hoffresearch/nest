"""Prove rust/ingest keeps the root's lint policy and toolchain floor.

rust/ingest is its own cargo workspace (the root excludes it), so it
inherits nothing: its `[lints]` tables are a copy of the root's
`[workspace.lints]`, and its `rust-version` a copy of the root's. a copy
drifts silently; this suite makes the drift a failure:

- happy path: every root lint is in rust/ingest at the same level and
  priority, and the two rust-version values agree;
- error path: a lint missing from the copy, or at another level, is named;
- edge case: rust/ingest may add a lint the root cannot have
  (`unsafe_code = "forbid"`: the engine's SIMD keeps the root from it).

Run: python tool/tests/test_lintmatch.py
"""

import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "Cargo.toml"
INGEST = REPO / "rust" / "ingest" / "Cargo.toml"


def load(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def drift(root: dict, copy: dict) -> list[str]:
    """what the copy lacks or sets differently, one line per lint."""
    found = []
    for tool, lints in root.items():
        theirs = copy.get(tool, {})
        for name, level in lints.items():
            if name not in theirs:
                found.append(f"{tool}::{name} missing from rust/ingest")
            elif theirs[name] != level:
                found.append(f"{tool}::{name} is {theirs[name]!r}, the root has {level!r}")
    return found


def test_ingest_copies_the_root_lints():
    root = load(ROOT)["workspace"]["lints"]
    copy = load(INGEST)["lints"]
    assert drift(root, copy) == [], drift(root, copy)


def test_ingest_keeps_the_root_rust_version():
    root = load(ROOT)["workspace"]["package"]["rust-version"]
    ingest = load(INGEST)["package"]["rust-version"]
    assert ingest == root, (ingest, root)


def test_a_drift_is_named():
    root = {"clippy": {"todo": "deny", "expect_used": "deny"}}
    copy = {"clippy": {"todo": "warn"}}
    assert drift(root, copy) == [
        "clippy::todo is 'warn', the root has 'deny'",
        "clippy::expect_used missing from rust/ingest",
    ], drift(root, copy)


def test_ingest_may_be_stricter():
    idioms = {"level": "deny", "priority": -1}
    root = {"rust": {"rust_2018_idioms": idioms}}
    copy = {"rust": {"rust_2018_idioms": dict(idioms), "unsafe_code": "forbid"}}
    assert drift(root, copy) == [], drift(root, copy)


def main() -> int:
    tests = [v for k, v in globals().items() if k.startswith("test_") and callable(v)]
    for test in tests:
        test()
        print(f"ok  {test.__name__}")
    print(f"lintmatch: {len(tests)} cases passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
