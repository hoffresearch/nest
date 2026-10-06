#!/bin/sh
# drive.sh - build, smoke and drive the urna CLI and its terminal explorer.
# run from the repo root, on macOS or Linux:
#   sh .devops/agent/skill/driveurna/drive.sh <command> [args]
#
#   ext               build the python extension (_urna) the forge imports
#   build             cargo build --release -p urna, then print its version
#   corpus [SPEC]     build a corpus from SPEC (default: the quickstart spec)
#   smoke             ask, retrieve, cite and validate on that corpus, then doctor
#   tui [FILE]        open `urna tui FILE` in tmux and wait for its first screen
#   key KEY...        tmux send-keys to the explorer (a, Tab, Enter, Down, C-q)
#   type TEXT         type literal text into the explorer
#   wait TEXT [SECS]  wait until TEXT is on the screen (default 20 s)
#   shot [NAME]       capture the screen to $SHOTS/NAME.txt and print it
#   quit              ctrl+q, then kill the session if it is still alive
#
# environment: URNA_BIN (the binary), URNA_FILE (the corpus), URNA_PYTHON
# (the python the extension is built for), CARGO_TARGET_DIR, SHOTS (where
# captures go), URNA_SESSION, URNA_COLS and URNA_ROWS (the tmux session).
set -eu

TARGET="${CARGO_TARGET_DIR:-target}"
U="${URNA_BIN:-$TARGET/release/urna}"
SPEC=demo/starter/corpus.toml
F="${URNA_FILE:-demos/quickstart/out/quickstart.urna}"
tmp="${TMPDIR:-/tmp}"
SHOTS="${SHOTS:-${tmp%/}/urna-shots}"
S="${URNA_SESSION:-urna}"
COLS="${URNA_COLS:-120}"
ROWS="${URNA_ROWS:-36}"
PY="${URNA_PYTHON:-python3}"
K=2

die() { printf 'drive: %s\n' "$*" >&2; exit 1; }
usage() { awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; }
need_bin() { [ -x "$U" ] || die "no $U; run: drive.sh build"; }
need_tmux() { command -v tmux > /dev/null || die "tmux not found; install it to drive the explorer"; }
need_session() { need_tmux; tmux has-session -t "$S" 2> /dev/null || die "no explorer running; run: drive.sh tui"; }
screen() { tmux capture-pane -t "$S" -p; }

# wait_for TEXT SECS [gone]: poll the screen every half second until TEXT
# shows up (or, with "gone", until it is no longer there).
wait_for() {
    tries=$(($2 * 2))
    while :; do
        if screen | grep -qF -- "$1"; then
            [ "${3:-}" = gone ] || return 0
        else
            [ "${3:-}" != gone ] || return 0
        fi
        tries=$((tries - 1))
        [ "$tries" -gt 0 ] || die "wait: \"$1\" ${3:+still}${3:-not} on screen after $2 s; see: drive.sh shot"
        sleep 0.5
    done
}

case "${1:-}" in
ext)
    case "$(uname -s)" in
    Darwin) lib=lib_urna.dylib ;;
    Linux) lib=lib_urna.so ;;
    *) die "ext: macOS and Linux only; elsewhere use the wheel (pip install urna)" ;;
    esac
    PYO3_PYTHON="$PY" cargo build --release -p urna-bridge --features pyo3/extension-module
    cp "$TARGET/release/$lib" rust/bridge/python/urna/_urna.so
    "$PY" -c 'import sys; sys.path.insert(0, "rust/bridge/python"); import urna; urna.UrnaFile' \
        || die "$PY cannot load rust/bridge/python/urna/_urna.so (the extension needs python 3.12 or newer)"
    echo "drive: rust/bridge/python/urna/_urna.so built for $PY"
    ;;
build)
    cargo build --release -p urna
    "$U" --version
    ;;
corpus)
    need_bin
    [ -f rust/bridge/python/urna/_urna.so ] || die "no rust/bridge/python/urna/_urna.so, which the forge imports; run: drive.sh ext"
    out=$("$U" build --spec "${2:-$SPEC}") || die "build failed for ${2:-$SPEC}"
    printf '%s\n' "$out" | "$PY" -c '
import json, sys
for o in json.load(sys.stdin)["outputs"].values():
    print("drive: built " + o["file"])'
    ;;
smoke)
    need_bin
    [ -f "$F" ] || die "no $F; run: drive.sh corpus"
    out=$("$U" ask "$F" "can I use this offline" -k 1 2> /dev/null) || die "ask failed"
    printf '%s\n' "$out" | grep -q '^  -- urna://sha256:' || die "ask printed no citation"
    echo "ok   ask: cited answer"
    pack=$("$U" retrieve "$F" "how do citations work" -k "$K" --format jsonl 2> /dev/null) || die "retrieve failed"
    [ "$(printf '%s\n' "$pack" | grep -c '"citation_id"')" -eq "$K" ] || die "retrieve: expected $K hits"
    echo "ok   retrieve: $K hits"
    cid=$(printf '%s\n' "$pack" | head -1 | sed 's/.*"citation_id":"\([^"]*\)".*/\1/')
    "$U" cite "$F" "$cid" | grep -q '^text' || die "cite did not resolve $cid"
    echo "ok   cite: round-trips $cid"
    "$U" validate "$F" > /dev/null || die "validate failed"
    echo "ok   validate"
    "$U" doctor > /dev/null 2>&1 || die "doctor failed (exit $?); run: urna setup --yes"
    echo "ok   doctor"
    ;;
tui)
    need_bin
    need_tmux
    file="${2:-$F}"
    [ -f "$file" ] || die "no $file; run: drive.sh corpus"
    tmux kill-session -t "$S" 2> /dev/null || true
    tmux new-session -d -s "$S" -x "$COLS" -y "$ROWS" "$U" tui "$file"
    # the explorer opens on the corpus tab with an "opening <file>" notice
    # over the section table; it is ready once the notice is gone.
    wait_for "sections" 20
    wait_for "opening " 20 gone
    echo "drive: explorer up in tmux session $S on $file"
    ;;
key)
    need_session
    shift
    tmux send-keys -t "$S" "$@"
    ;;
type)
    need_session
    shift
    tmux send-keys -t "$S" -l "$*"
    ;;
wait)
    need_session
    [ -n "${2:-}" ] || die "usage: drive.sh wait TEXT [SECS]"
    wait_for "$2" "${3:-20}"
    ;;
shot)
    need_session
    mkdir -p "$SHOTS"
    name="${2:-shot-$(date +%H%M%S)}"
    screen > "$SHOTS/$name.txt"
    cat "$SHOTS/$name.txt"
    echo "drive: saved $SHOTS/$name.txt"
    ;;
quit)
    need_tmux
    tmux send-keys -t "$S" C-q 2> /dev/null || true
    sleep 1
    tmux kill-session -t "$S" 2> /dev/null || true
    echo "drive: explorer closed"
    ;;
*)
    usage
    exit 2
    ;;
esac
