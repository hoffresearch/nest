#!/bin/sh
# drive.sh - build, smoke and drive the urna CLI and its terminal explorer.
# run from the repo root:
#   sh .contracts/.ai/.agents/.skills/run-urna/drive.sh <command> [args]
#
#   build            cargo build --release -p urna (the binary `urna`)
#   corpus           build demos/quickstart/out/quickstart.urna from its spec
#   smoke            the five-verb loop on that corpus + doctor; fails on any miss
#   tui [FILE]       open `urna tui FILE` in tmux session `urna` (120x36)
#   key KEY...       tmux send-keys to the explorer (a, Tab, Enter, Down, C-q)
#   type TEXT        type literal text into the explorer
#   shot [NAME]      capture the screen to $SHOTS/NAME.txt and print it
#   quit             ctrl+q, then kill the session if it is still alive
set -eu

U="${URNA_BIN:-target/release/urna}"
F="${URNA_FILE:-demos/quickstart/out/quickstart.urna}"
SHOTS="${SHOTS:-/tmp/urna-shots}"
S=urna

die() { printf 'drive: %s\n' "$*" >&2; exit 1; }
need_bin() { [ -x "$U" ] || die "no $U; run: drive.sh build"; }

case "${1:-}" in
build)
    cargo build --release -p urna
    "$U" --version
    ;;
corpus)
    need_bin
    rm -rf demos/quickstart/out
    "$U" build --spec demos/quickstart/corpus.toml > /dev/null
    [ -f "$F" ] || die "build wrote no $F"
    echo "drive: built $F"
    ;;
smoke)
    need_bin
    [ -f "$F" ] || die "no $F; run: drive.sh corpus"
    out=$("$U" ask "$F" "can I use this offline" -k 1 2>/dev/null)
    echo "$out" | grep -q '^  -- urna://sha256:' || die "ask printed no citation"
    echo "ok   ask: cited answer"
    pack=$("$U" retrieve "$F" "how do citations work" -k 2 --format jsonl 2>/dev/null)
    [ "$(printf '%s\n' "$pack" | grep -c '"citation_id"')" -eq 2 ] || die "retrieve: expected 2 hits"
    echo "ok   retrieve: 2 hits"
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
    file="${2:-$F}"
    tmux kill-session -t "$S" 2>/dev/null || true
    tmux new-session -d -s "$S" -x 120 -y 36 "$U tui $file"
    sleep 4  # the open toast covers the section table for ~3 s
    echo "drive: explorer up in tmux session $S on $file"
    ;;
key)
    shift
    tmux send-keys -t "$S" "$@"
    ;;
type)
    shift
    tmux send-keys -t "$S" -l "$*"
    ;;
shot)
    mkdir -p "$SHOTS"
    name="${2:-shot-$(date +%H%M%S)}"
    tmux capture-pane -t "$S" -p > "$SHOTS/$name.txt"
    cat "$SHOTS/$name.txt"
    echo "drive: saved $SHOTS/$name.txt"
    ;;
quit)
    tmux send-keys -t "$S" C-q 2>/dev/null || true
    sleep 1
    tmux kill-session -t "$S" 2>/dev/null || true
    echo "drive: explorer closed"
    ;;
*)
    sed -n '2,15p' "$0"
    exit 2
    ;;
esac
