"""What each release channel serves for one exact version.

Two subcommands share one probe per channel:

- ``wait --channel npm|crates|pypi --version X.Y.Z``: install-test runs it
  before installing from a registry, so a channel that is still propagating
  is waited for (bounded, ``--timeout``) instead of failing the leg. Only the
  exact version counts: ``0.5.30`` is not ``0.5.3``.
- ``crate --name N --version X.Y.Z``: publish-crates asks it whether to
  publish: exit 0 published (skip), 10 absent (publish), 1 when crates.io
  answers 429, a 5xx or nothing after the retries (stop, never publish).
- ``report --run ID --sha SHA --tag vX.Y.Z``: release-report.yml runs it when
  a release run completes, whatever its conclusion. It writes one summary:
  the run and every job that did not succeed, whether the tag points at the
  run's commit, the pypi.yml run for that commit, and what the GitHub
  release, crates.io, npm, Homebrew and PyPI serve. The summary is written
  first; the exit code then fails the report when anything is missing.

Endpoints (overridable for tests through the environment):

- npm: ``$NPM_REGISTRY/@urna%2fcli/<version>`` (the version document);
- crates.io: ``$CRATES_INDEX/ur/na/urna`` (the sparse index cargo and
  binstall resolve through), a non-yanked line with that ``vers``;
- PyPI: ``$PYPI_URL/pypi/urna/<version>/json``, with files;
- Homebrew: ``$RAW_GITHUB/hoffresearch/homebrew-urna/main/Formula/urna.rb``;
- GitHub: ``$GITHUB_API_URL`` (release, run, jobs, tag ref), ``GH_TOKEN``.

    python scripts/release_channels.py wait --channel npm --version 0.5.3
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

UA = "urna-release (github.com/hoffresearch/urna)"
SERVED = "served"


def _env(name: str, default: str) -> str:
    return os.environ.get(name, default).rstrip("/")


def _get(url: str, token: bool = False) -> tuple[int, str]:
    """(status, body); status 0 when the host does not answer."""
    headers = {"User-Agent": UA, "Accept": "application/json"}
    if token and (tok := os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")):
        headers["Authorization"] = f"Bearer {tok}"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=20) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as err:
        return err.code, ""
    except (urllib.error.URLError, TimeoutError, OSError) as err:
        return 0, str(err)


def _state(status: int, ok: bool, what: str) -> str:
    if status == 200 and ok:
        return SERVED
    if status in (200, 404):
        return f"absent ({what})"
    return f"unavailable (HTTP {status or 'no answer'})"


def probe_npm(version: str) -> str:
    status, body = _get(
        f"{_env('NPM_REGISTRY', 'https://registry.npmjs.org')}/@urna%2fcli/{version}"
    )
    doc = json.loads(body) if status == 200 and body else {}
    return _state(status, doc.get("version") == version, f"npm has no @urna/cli {version}")


def probe_crates(version: str) -> str:
    status, body = _get(f"{_env('CRATES_INDEX', 'https://index.crates.io')}/ur/na/urna")
    lines = [json.loads(ln) for ln in body.splitlines() if ln.strip()] if status == 200 else []
    ok = any(ln.get("vers") == version and not ln.get("yanked") for ln in lines)
    return _state(status, ok, f"the index has no unyanked urna {version}")


def probe_pypi(version: str) -> str:
    status, body = _get(f"{_env('PYPI_URL', 'https://pypi.org')}/pypi/urna/{version}/json")
    doc = json.loads(body) if status == 200 and body else {}
    files = doc.get("urls") or []
    ok = doc.get("info", {}).get("version") == version and bool(files)
    state = _state(status, ok, f"pypi has no urna {version} files")
    return f"{SERVED} ({len(files)} files)" if state == SERVED else state


def probe_homebrew(version: str) -> str:
    raw = _env("RAW_GITHUB", "https://raw.githubusercontent.com")
    status, body = _get(f"{raw}/hoffresearch/homebrew-urna/main/Formula/urna.rb")
    found = re.search(r'^\s*version "([^"]+)"', body, re.M) if status == 200 else None
    at = found.group(1) if found else None
    return _state(status, at == version, f"the formula is at {at}")


def probe_github(version: str, repo: str) -> str:
    api = _env("GITHUB_API_URL", "https://api.github.com")
    status, body = _get(f"{api}/repos/{repo}/releases/tags/v{version}", token=True)
    assets = json.loads(body).get("assets", []) if status == 200 and body else []
    state = _state(status, bool(assets), f"no release v{version}")
    return f"{SERVED} ({len(assets)} assets)" if state == SERVED else state


PROBES = {"npm": probe_npm, "crates": probe_crates, "pypi": probe_pypi}

PUBLISHED, ABSENT = 0, 10


def crate_status(name: str, version: str, attempts: int = 5, delay: float = 5) -> tuple[int, str]:
    """(PUBLISHED | ABSENT | 1, why) for one crate version on crates.io.

    Only a 200 means published and only a 404 means absent. A 429, a 5xx or
    no answer is retried, then reported as an error: publish-crates must not
    read an outage as "not published yet" and try to publish over it.
    """
    api = _env("CRATES_API", "https://crates.io/api/v1")
    for attempt in range(1, attempts + 1):
        status, _ = _get(f"{api}/crates/{name}/{version}")
        if status == 200:
            return PUBLISHED, f"{name} {version} is on crates.io"
        if status == 404:
            return ABSENT, f"{name} {version} is not on crates.io"
        why = f"crates.io answered {status or 'nothing'} for {name} {version}"
        print(f"release-channels: {why} (attempt {attempt} of {attempts})", flush=True)
        if attempt < attempts:
            time.sleep(delay)
            delay = min(delay * 2, 60)
    return 1, why


def wait(channel: str, version: str, timeout: float, interval: float) -> tuple[bool, str]:
    deadline, delay = time.monotonic() + timeout, interval
    while True:
        state = PROBES[channel](version)
        print(f"release-channels: {channel} {version}: {state}", flush=True)
        if state.startswith(SERVED):
            return True, state
        if time.monotonic() + delay > deadline:
            return False, f"{channel} did not serve {version} within {timeout:.0f}s: {state}"
        time.sleep(delay)
        delay = min(delay * 2, 60)


def _json(url: str, token: bool = False) -> dict:
    status, body = _get(url, token)
    return json.loads(body) if status == 200 and body else {}


def wheels_and_attestations(version: str, repo: str) -> tuple[list[str], list[str]]:
    """(summary lines, problems): the release's wheels against PyPI's, and an
    attestation for every archive and wheel on the release (by digest)."""
    api = _env("GITHUB_API_URL", "https://api.github.com")
    assets = _json(f"{api}/repos/{repo}/releases/tags/v{version}", token=True).get("assets", [])
    digest = {a["name"]: str(a.get("digest") or "").removeprefix("sha256:") for a in assets}
    pypi = _json(f"{_env('PYPI_URL', 'https://pypi.org')}/pypi/urna/{version}/json")
    on_pypi = {f["filename"]: f["digests"]["sha256"] for f in pypi.get("urls", [])}
    on_release = {n: d for n, d in digest.items() if n.endswith(".whl")}
    lines, problems = [], []
    if not on_release:
        problems.append("the GitHub release carries no wheels")
    elif on_release != on_pypi:
        differ = sorted(
            n for n in set(on_release) | set(on_pypi) if on_release.get(n) != on_pypi.get(n)
        )
        problems.append(f"PyPI and the GitHub release differ on {differ}")
    else:
        lines.append(f"Wheels: PyPI and the GitHub release carry the same {len(on_release)} files")
    signed = [n for n in digest if n.endswith((".tar.xz", ".zip", ".whl"))]
    missing = [
        n
        for n in sorted(signed)
        if not digest[n]
        or not _json(f"{api}/repos/{repo}/attestations/sha256:{digest[n]}", token=True).get(
            "attestations"
        )
    ]
    if missing:
        problems.append(f"no attestation for {missing}")
    elif signed:
        lines.append(
            f"Attestations: every archive and wheel on the release has one ({len(signed)})"
        )
    return lines, problems


def _tag_commit(api: str, repo: str, tag: str) -> str | None:
    status, body = _get(f"{api}/repos/{repo}/git/ref/tags/{tag}", token=True)
    obj = json.loads(body).get("object", {}) if status == 200 and body else {}
    if obj.get("type") == "tag":  # annotated: one more hop to the commit
        status, body = _get(f"{api}/repos/{repo}/git/tags/{obj.get('sha')}", token=True)
        obj = json.loads(body).get("object", {}) if status == 200 and body else {}
    return obj.get("sha")


def report(run_id: str, sha: str, tag: str, repo: str) -> tuple[bool, str]:
    """The markdown summary and whether everything was released."""
    api = _env("GITHUB_API_URL", "https://api.github.com")
    version = tag.removeprefix("v")
    problems: list[str] = []
    status, body = _get(f"{api}/repos/{repo}/actions/runs/{run_id}", token=True)
    run = json.loads(body) if status == 200 and body else {}
    conclusion = run.get("conclusion") or f"unknown (HTTP {status})"
    if conclusion != "success":
        problems.append(f"the release run ended {conclusion}")
    lines = [f"## Release {tag}", "", f"Release run {run_id}: **{conclusion}** at `{sha[:12]}`", ""]
    commit = _tag_commit(api, repo, tag)
    if commit != sha:
        problems.append(f"tag {tag} points at {commit}, not the run's {sha[:12]}")
    lines.append(
        f"Tag {tag} -> `{(commit or 'missing')[:12]}`"
        + ("" if commit == sha else " (not this run)")
    )
    status, body = _get(f"{api}/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100", token=True)
    jobs = json.loads(body).get("jobs", []) if status == 200 and body else []
    off = [j for j in jobs if j.get("conclusion") not in ("success",)]
    lines += ["", f"Jobs: {len(jobs) - len(off)} of {len(jobs)} succeeded"]
    lines += [f"- {j.get('name')}: {j.get('conclusion') or j.get('status')}" for j in off]
    status, body = _get(
        f"{api}/repos/{repo}/actions/workflows/pypi.yml/runs?head_sha={sha}", token=True
    )
    pypi_runs = json.loads(body).get("workflow_runs", []) if status == 200 and body else []
    pypi_run = pypi_runs[0] if pypi_runs else {}
    pypi_state = pypi_run.get("conclusion") or pypi_run.get("status") or "no run for this commit"
    lines.append(f"pypi.yml: {pypi_state}")
    channels = {
        "GitHub release": probe_github(version, repo),
        "crates.io": probe_crates(version),
        "npm": probe_npm(version),
        "Homebrew": probe_homebrew(version),
        "PyPI": probe_pypi(version),
    }
    lines += ["", "| channel | " + version + " |", "|---|---|"]
    lines += [f"| {name} | {state} |" for name, state in channels.items()]
    problems += [
        f"{name}: {state}" for name, state in channels.items() if not state.startswith(SERVED)
    ]
    extra, issues = wheels_and_attestations(version, repo)
    lines += ["", *extra]
    problems += issues
    lines += ["", "Released everywhere." if not problems else "Not released everywhere:"]
    lines += [f"- {p}" for p in problems]
    return not problems, "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    w = sub.add_parser("wait")
    w.add_argument("--channel", choices=sorted(PROBES), required=True)
    w.add_argument("--version", required=True)
    w.add_argument("--timeout", type=float, default=900)
    w.add_argument("--interval", type=float, default=10)
    c = sub.add_parser("crate")
    c.add_argument("--name", required=True)
    c.add_argument("--version", required=True)
    c.add_argument("--attempts", type=int, default=5)
    c.add_argument("--delay", type=float, default=5)
    r = sub.add_parser("report")
    r.add_argument("--run", required=True)
    r.add_argument("--sha", required=True)
    r.add_argument("--tag", required=True)
    r.add_argument("--repo", default="hoffresearch/urna")
    args = parser.parse_args(argv)
    if args.cmd == "wait":
        ok, why = wait(args.channel, args.version.removeprefix("v"), args.timeout, args.interval)
        print(f"release-channels: {why}", file=sys.stdout if ok else sys.stderr)
        return 0 if ok else 1
    if args.cmd == "crate":
        code, why = crate_status(args.name, args.version, args.attempts, args.delay)
        print(f"release-channels: {why}", file=sys.stderr if code == 1 else sys.stdout)
        return code
    ok, text = report(args.run, args.sha, args.tag, args.repo)
    print(text)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(text)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
