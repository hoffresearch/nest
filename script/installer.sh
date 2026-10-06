#!/bin/sh
# temporary stub (#479): the installer moved to tool/tasks/installer.sh. the
# README and the published crates.io, PyPI and npm pages still point here
# until the next release, so this fetches the moved script and runs it with
# the same arguments. a `curl ... | sh` has no checkout, so it cannot call a
# relative path. removed in the first release after the move.
set -eu
URL="https://raw.githubusercontent.com/hoffresearch/urna/main/tool/tasks/installer.sh"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
curl -sSfL "$URL" -o "$tmp"
sh "$tmp" "$@"
