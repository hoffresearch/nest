"""Manifest v1 (RFC-0 N9), build.lock.json (N1) and provenance redaction (N10).

The manifest is a versioned contract, not an ad-hoc log: required fields are
asserted at write time, serialization is canonical (sort_keys), and readers
must check `manifest_schema_version` first. The build lock pins everything an
L3 (byte-identical) claim depends on; `--rebuild-only` compares against it.
"""

from __future__ import annotations

import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path

from forge.forge_cache import atomic_write_bytes

MANIFEST_SCHEMA_VERSION = 1
_REQUIRED = (
    "manifest_schema_version",
    "name",
    "n_items",
    "n_unique_frames",
    "models",
    "spaces",
    "media",
    "provenance_mode",
    "timings",
)


def redact_path(value: str, mode: str, spec_dir: Path) -> str:
    if mode == "full":
        return value
    home = str(Path.home())
    out = value.replace(home, "~") if value.startswith(home) else value
    if mode == "minimal":
        return Path(out).name
    try:
        return str(Path(out).relative_to(spec_dir))
    except ValueError:
        return out


def write_manifest(path: Path, payload: dict, mode: str, spec_dir: Path) -> None:
    payload = dict(payload, manifest_schema_version=MANIFEST_SCHEMA_VERSION, provenance_mode=mode)
    if mode == "minimal":
        payload.pop("sql", None)
        for item in payload.get("items", []):
            item.pop("label", None)
    missing = [k for k in _REQUIRED if k not in payload]
    if missing:
        raise ValueError(f"manifest missing required fields: {missing}")
    atomic_write_bytes(path, json.dumps(payload, sort_keys=True, indent=1).encode())


def _tool_fingerprint(name: str) -> dict | None:
    exe = shutil.which(name)
    if not exe:
        return None
    try:
        out = subprocess.run([exe, "-version"], capture_output=True, text=True, timeout=10)
        text = out.stdout or out.stderr
        version = text.splitlines()[0].strip() if text else ""
    except (subprocess.SubprocessError, OSError):
        version = ""
    return {
        "path": exe,
        "version": version,
        "sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(),
    }


def _package_versions() -> dict[str, str]:
    import importlib.metadata as md

    versions = {"python": sys.version.split()[0]}
    import contextlib

    for pkg in (
        "numpy",
        "torch",
        "transformers",
        "sentence-transformers",
        "open-clip-torch",
        "tokenizers",
        "pillow",
    ):
        with contextlib.suppress(md.PackageNotFoundError):
            versions[pkg] = md.version(pkg)
    return versions


def build_lock(spec, model_hashes: dict[str, str], device: str) -> dict:
    return {
        "lock_schema_version": 1,
        "platform": {
            "os": platform.system(),
            "machine": platform.machine(),
            "release": platform.release(),
        },
        "packages": _package_versions(),
        "tools": {
            name: _tool_fingerprint(name)
            for name in ("ffmpeg", "ffprobe", "cjxl", "djxl", "ssimulacra2")
        },
        "models": model_hashes,
        "device": device,
        "resolved_spec": _spec_dict(spec),
    }


def _spec_dict(spec) -> dict:
    raw = asdict(spec)
    raw["media"] = asdict(spec.media) if spec.media is not None else None
    return raw


def check_lock(previous: dict, current: dict) -> list[str]:
    """Return human-readable divergences between two build locks (L3 gate)."""
    diffs: list[str] = []

    def walk(prefix: str, a, b) -> None:
        if isinstance(a, dict) and isinstance(b, dict):
            for k in sorted(set(a) | set(b)):
                walk(f"{prefix}.{k}" if prefix else k, a.get(k), b.get(k))
        elif a != b:
            diffs.append(f"{prefix}: {_short(a)} -> {_short(b)}")

    walk("", previous, current)
    return [d for d in diffs if not re.match(r"resolved_spec\.(spec_path|output\.dir)", d)]


def _short(v) -> str:
    s = json.dumps(v, sort_keys=True, default=str)
    return s if len(s) <= 60 else s[:57] + "..."
