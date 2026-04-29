---
name: bug report
about: report a defect in the nest format, runtime, cli, or python bindings
title: ''
labels: bug
assignees: ''
---

<!-- title in plain english, no Conventional Commits prefix. body explains what is broken and why it matters. -->

## summary

<!-- 1-3 sentences: what is broken. -->

## reproduction

<!-- minimal repro. paste the exact command, code, or .nest path. include input and any relevant flags. -->

```
$ nest <command> ...
```

## expected vs actual

<!-- what should happen, then what actually happens. paste the actual error or output verbatim. -->

- expected:
- actual:

## environment

- nest version / commit:
- os + arch:
- rust toolchain (`rustc --version`):
- python (`python3 --version`):
- embedding model (if relevant):

## scope

- [ ] binary format / hash semantics
- [ ] runtime / search contract
- [ ] cli
- [ ] python bindings
- [ ] build / release tooling
- [ ] docs only

## additional context

<!-- logs, related issues, prior ADRs, anything else. n/a if none. -->
