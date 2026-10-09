"""Prove the gate's corpus is fetched only when allowed and measured only if pinned.

`tool/tasks/benchdata.py` resolves the corpus fullcheck measures. this suite
runs it against a throwaway hugging face cache (HF_HOME), with no network:

- happy path: a cached file with the pinned sha-256 is returned as is,
  without URNA_ALLOW_DOWNLOAD;
- error path: no file and no URNA_ALLOW_DOWNLOAD stops with exit 3 and the
  command that fetches it; a cached file with another sha-256 is refused;
- edge case: the path is where hf_hub_download puts a dataset file at the
  pinned revision, under $HF_HOME/hub.

Run: python tool/tests/test_benchdata.py
"""

import contextlib
import hashlib
import importlib.util
import io
import os
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "tool" / "tasks" / "benchdata.py"
spec = importlib.util.spec_from_file_location("benchdata", SCRIPT)
benchdata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchdata)


@contextlib.contextmanager
def cache(allow: bool):
    keep = {k: os.environ.get(k) for k in ("HF_HOME", "URNA_ALLOW_DOWNLOAD")}
    with tempfile.TemporaryDirectory(prefix="urna-benchdata-") as home:
        os.environ["HF_HOME"] = home
        os.environ.pop("URNA_ALLOW_DOWNLOAD", None)
        if allow:
            os.environ["URNA_ALLOW_DOWNLOAD"] = "1"
        try:
            yield Path(home)
        finally:
            for k, v in keep.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v


def place(data: bytes) -> Path:
    p = benchdata.path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data)
    return p


def test_path_is_the_hub_layout():
    with cache(allow=False) as home:
        want = (
            home
            / "hub/datasets--brennercruvinel--fakenews-ptbr-urna-benchmark/snapshots"
            / benchdata.REVISION
            / "release/v0.1/minilm-exact/fakenews.urna"
        )
        assert benchdata.path() == want, benchdata.path()


def test_missing_without_consent_names_the_command():
    with cache(allow=False):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = benchdata.main(["fetch"])
        assert code == 3, code
        assert benchdata.command() in err.getvalue(), err.getvalue()
        assert not benchdata.path().exists()


def test_pinned_file_is_used_without_consent():
    pinned = benchdata.SHA256
    data = b"a corpus"
    benchdata.SHA256 = hashlib.sha256(data).hexdigest()
    try:
        with cache(allow=False):
            p = place(data)
            assert benchdata.fetch() == p
    finally:
        benchdata.SHA256 = pinned


def test_another_file_is_refused():
    with cache(allow=True):
        place(b"not the corpus")
        try:
            benchdata.fetch()
        except benchdata.Refused as e:
            assert e.code == 6, e.code
            assert "not the pinned" in str(e), e
        else:
            raise AssertionError("a file with another sha-256 was accepted")


def main() -> int:
    tests = [v for k, v in globals().items() if k.startswith("test_") and callable(v)]
    for test in tests:
        test()
        print(f"ok  {test.__name__}")
    print(f"benchdata: {len(tests)} cases passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
