"""install_model.py - fetch one catalog model into the local hugging face cache.

the second half of the one install operation `urna setup` and the explorer
share (`crates/urna-cli/src/cmd/models.rs`): the rust side installs the
packages into the managed venv, then runs this script with that venv's
python to fetch the weights and prove them.

  <py> install_model.py plan  <name|embedding_model> [--expect-hash H]
  <py> install_model.py fetch <name|embedding_model> [--expect-hash H]
                                                  [--allow-remote-code]

plan prints one JSON line: the entry, the bytes still to fetch, whether the
pinned revision is already cached and which packages do not import here.
it never touches the network.

fetch downloads exactly the catalog's files at its pinned revision into the
shared cache (`$HF_HOME/hub`, the cache the query embedders read), then
fingerprints them and refuses unless the result is the catalog's
model_hash (and the corpus's, with --expect-hash). the network is used only
with URNA_ALLOW_DOWNLOAD=1, which the caller sets after the user confirmed
the download; a model that runs repo code also needs --allow-remote-code, a
separate consent, checked before anything is fetched. progress goes to
stdout as `urna-progress: <done> <total> <file>` lines.

exit codes: 0 ok, 2 usage, 3 download not allowed (`urna-fetch: <repo>`),
4 a package missing (`urna-needs: <spec>`), 5 remote code not allowed
(`urna-consent: remote-code <name>`), 6 a hash or revision that does not
match, 7 a model the catalog does not offer, 8 the download failed.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import sys
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

CATALOG = HERE / "catalog.json"


class Refused(Exception):
    def __init__(self, code: int, msg: str, tag: str | None = None):
        super().__init__(msg)
        self.code = code
        self.tag = tag


def load_catalog(path: Path = CATALOG) -> dict:
    return json.loads(path.read_text())


def find(catalog: dict, name: str) -> dict:
    """The offered entry for a catalog name or a manifest model name."""
    for m in catalog["models"]:
        if name in (m["name"], m["embedding_model"]):
            return m
    for e in catalog["excluded"]:
        if name in (e["name"], e["embedding_model"]):
            raise Refused(7, f"{e['name']} is not offered for install: {e['reason']}")
    offered = ", ".join(m["name"] for m in catalog["models"]) or "none"
    raise Refused(7, f"{name} is not in the model catalog (offered: {offered})")


def hub_dir() -> Path:
    """The cache the query embedders read (model_fingerprint.hf_cache_snapshot):
    $HF_HOME/hub, else ~/.cache/huggingface/hub."""
    home = os.environ.get("HF_HOME") or str(Path.home() / ".cache" / "huggingface")
    return Path(home) / "hub"


def repo_dir(entry: dict) -> Path:
    return hub_dir() / f"models--{entry['repo'].replace('/', '--')}"


def snapshot(entry: dict) -> Path:
    return repo_dir(entry) / "snapshots" / entry["revision"]


def missing_files(entry: dict) -> list[dict]:
    snap = snapshot(entry)
    return [f for f in entry["files"] if not (snap / f["path"]).is_file()]


def missing_packages(entry: dict) -> list[str]:
    if all(importlib.util.find_spec(m) is not None for m in entry["imports"]):
        return []
    return list(entry["packages"])


def check_expected(entry: dict, expect: str | None) -> None:
    if expect and expect != entry["model_hash"]:
        raise Refused(
            6,
            f"{entry['name']} at revision {entry['revision'][:12]} fingerprints to "
            f"{entry['model_hash']}, but the corpus was built with {expect}: installing "
            "it would not make this corpus askable",
        )


def plan(entry: dict, expect: str | None) -> dict:
    check_expected(entry, expect)
    todo = missing_files(entry)
    return {
        "name": entry["name"],
        "repo": entry["repo"],
        "revision": entry["revision"],
        "bytes": entry["bytes"],
        "missing_bytes": sum(f["size"] for f in todo),
        "cached": not todo,
        "packages_missing": missing_packages(entry),
        "remote_code": entry["remote_code"],
        "model_hash": entry["model_hash"],
        "python": sys.executable,
        "cache": str(hub_dir()),
    }


def _progress(done: int, total: int, path: str) -> None:
    print(f"urna-progress: {done} {total} {path}", flush=True)


def _watch(entry: dict, base: int, total: int, path: str, stop: threading.Event) -> None:
    """Reports the partial blob's size while one file downloads. the hub
    client writes it under the repo's blobs/ or, since huggingface_hub 1.x,
    under the cache-wide blobs/<xx>/ store."""
    repo_blobs, shared = repo_dir(entry) / "blobs", hub_dir() / "blobs"
    last = -1
    while not stop.wait(0.5):
        parts = [*repo_blobs.glob("*.incomplete"), *shared.glob("*/*.incomplete")]
        part = sum(p.stat().st_size for p in parts if p.is_file())
        if part != last:
            last = part
            _progress(base + part, total, path)


def download(entry: dict) -> None:
    # the progress lines above replace the client's bars and its warnings.
    os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")
    os.environ.setdefault("HF_HUB_VERBOSITY", "error")
    if importlib.util.find_spec("huggingface_hub") is None:
        raise Refused(4, "fetching a model needs huggingface_hub", "urna-needs: huggingface_hub")
    from huggingface_hub import hf_hub_download

    total = entry["bytes"]
    todo = {f["path"] for f in missing_files(entry)}
    done = total - sum(f["size"] for f in entry["files"] if f["path"] in todo)
    for f in entry["files"]:
        if f["path"] not in todo:
            continue
        _progress(done, total, f["path"])
        stop = threading.Event()
        watch = threading.Thread(
            target=_watch, args=(entry, done, total, f["path"], stop), daemon=True
        )
        watch.start()
        try:
            hf_hub_download(
                entry["repo"],
                f["path"],
                revision=entry["revision"],
                cache_dir=str(hub_dir()),
            )
        except Exception as e:
            raise Refused(8, f"downloading {f['path']} from {entry['repo']} failed: {e}") from e
        finally:
            stop.set()
            watch.join()
        done += f["size"]
        _progress(done, total, f["path"])


def pin_main_ref(entry: dict) -> None:
    """Point refs/main at the pinned revision when the cache has no main yet:
    sentence-transformers and the fingerprint load what refs/main names. a
    main that already names another revision belongs to whoever fetched it;
    it is left alone and the run fails naming both."""
    ref = repo_dir(entry) / "refs" / "main"
    if not ref.is_file():
        ref.parent.mkdir(parents=True, exist_ok=True)
        ref.write_text(entry["revision"])
        return
    held = ref.read_text().strip()
    if held != entry["revision"]:
        raise Refused(
            6,
            f"the cache's main for {entry['repo']} is {held[:12]}, the catalog pins "
            f"{entry['revision'][:12]}; queries load main, so they would not get the "
            f"pinned model. the pinned files are at {snapshot(entry)} (--model-path)",
        )


def verify(entry: dict) -> str:
    snap = snapshot(entry)
    for pin in entry["remote_code_hashes"]:
        got = hashlib.sha256((snap / pin["path"]).read_bytes()).hexdigest()
        if got != pin["sha256"]:
            raise Refused(6, f"{pin['path']} sha256 {got} is not the reviewed {pin['sha256']}")
    from model_fingerprint import compute_model_fingerprint, fingerprint_to_model_hash

    fp = compute_model_fingerprint(snap, model_id=entry["embedding_model"])
    got = fingerprint_to_model_hash(fp)
    if got != entry["model_hash"]:
        raise Refused(
            6,
            f"{entry['name']} at {entry['revision'][:12]} fingerprints to {got}, "
            f"not the catalog's {entry['model_hash']}",
        )
    return got


def fetch(entry: dict, expect: str | None, allow_remote_code: bool) -> dict:
    check_expected(entry, expect)
    if entry["remote_code"] and not allow_remote_code:
        raise Refused(
            5,
            f"{entry['name']} runs code from its model repo; that needs its own consent",
            f"urna-consent: remote-code {entry['name']}",
        )
    if missing_files(entry):
        if os.environ.get("URNA_ALLOW_DOWNLOAD") != "1":
            raise Refused(
                3,
                f"{entry['repo']} is not in the local cache and downloads were not confirmed",
                f"urna-fetch: {entry['repo']}",
            )
        download(entry)
    pin_main_ref(entry)
    model_hash = verify(entry)
    return {"name": entry["name"], "model_hash": model_hash, "snapshot": str(snapshot(entry))}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("action", choices=("plan", "fetch"))
    ap.add_argument("model")
    ap.add_argument("--expect-hash")
    ap.add_argument("--allow-remote-code", action="store_true")
    ap.add_argument("--catalog", type=Path, default=CATALOG)
    args = ap.parse_args(argv)
    try:
        entry = find(load_catalog(args.catalog), args.model)
        if args.action == "plan":
            out = plan(entry, args.expect_hash)
        else:
            out = fetch(entry, args.expect_hash, args.allow_remote_code)
    except Refused as e:
        print(f"error: {e}", file=sys.stderr)
        if e.tag:
            print(e.tag, file=sys.stderr)
        return e.code
    print(json.dumps(out), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
