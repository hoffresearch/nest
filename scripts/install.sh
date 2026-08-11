#!/bin/sh
# nest installer (issue #75). one-liner:
#
#   curl -sSf https://get.hoffresearch.com/nest | sh
#
# downloads the release tarball for this platform, verifies its sha256
# against the release's checksum file, installs the binary to
# ~/.local/bin, and lays down the offline embedder payload (potion table)
# under ${XDG_DATA_HOME:-~/.local/share}/nest. after install the product
# never touches the network; only this script does.
#
# flags:
#   --version vX.Y.Z   pin a release (default: latest)
#   --uninstall        remove the binary and the payload
#
# env overrides (used by tests and custom setups):
#   NEST_RELEASE_BASE  url prefix that serves the release artifacts
#   NEST_BIN_DIR       binary install dir      (default ~/.local/bin)
#   NEST_DATA_DIR      payload parent dir      (default ${XDG_DATA_HOME:-~/.local/share})

set -eu

REPO="hoffresearch/nest"

say() { printf '%s\n' "$*"; }
die() { printf 'nest-install: error: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

VERSION=""
UNINSTALL=0
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            [ $# -ge 2 ] || die "--version needs a value"
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#--version=}"
            shift
            ;;
        --uninstall)
            UNINSTALL=1
            shift
            ;;
        -h | --help)
            say "usage: install.sh [--version vX.Y.Z] [--uninstall]"
            exit 0
            ;;
        *) die "unknown flag: $1" ;;
    esac
done
case "$VERSION" in
    "" | v*) ;;
    *) VERSION="v$VERSION" ;;
esac

BIN_DIR="${NEST_BIN_DIR:-$HOME/.local/bin}"
DATA_DIR="${NEST_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}}"
PAYLOAD_DIR="$DATA_DIR/nest"

if [ "$UNINSTALL" -eq 1 ]; then
    rm -f "$BIN_DIR/nest"
    rm -rf "$PAYLOAD_DIR"
    say "nest-install: removed $BIN_DIR/nest and $PAYLOAD_DIR"
    exit 0
fi

need curl
need tar

# platform detection -> release target triple.
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux) OS_PART="unknown-linux-musl" ;;
    Darwin) OS_PART="apple-darwin" ;;
    *) die "unsupported os: $OS (use scripts/install.ps1 on windows)" ;;
esac
case "$ARCH" in
    x86_64 | amd64) ARCH_PART="x86_64" ;;
    arm64 | aarch64) ARCH_PART="aarch64" ;;
    *) die "unsupported arch: $ARCH" ;;
esac
TARGET="$ARCH_PART-$OS_PART"

if [ -n "${NEST_RELEASE_BASE:-}" ]; then
    BASE="$NEST_RELEASE_BASE"
elif [ -n "$VERSION" ]; then
    BASE="https://github.com/$REPO/releases/download/$VERSION"
else
    BASE="https://github.com/$REPO/releases/latest/download"
fi

ARCHIVE="nest-cli-$TARGET.tar.xz"
PAYLOAD="nest-embedder-payload.tar.gz"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say "nest-install: fetching $ARCHIVE + payload ($TARGET)"
curl -sSfL "$BASE/$ARCHIVE" -o "$TMP/$ARCHIVE" || die "download failed: $BASE/$ARCHIVE"
curl -sSfL "$BASE/$ARCHIVE.sha256" -o "$TMP/$ARCHIVE.sha256" || die "checksum download failed"
curl -sSfL "$BASE/$PAYLOAD" -o "$TMP/$PAYLOAD" || die "download failed: $BASE/$PAYLOAD"
curl -sSfL "$BASE/$PAYLOAD.sha256" -o "$TMP/$PAYLOAD.sha256" || die "checksum download failed"

# verify both sha256 sums before anything touches the install dirs.
verify() {
    file="$1"
    sumfile="$2"
    want="$(awk '{print $1}' "$sumfile")"
    [ -n "$want" ] || die "empty checksum in $sumfile"
    if command -v sha256sum >/dev/null 2>&1; then
        got="$(sha256sum "$file" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        got="$(shasum -a 256 "$file" | awk '{print $1}')"
    else
        die "no sha256 tool (sha256sum/shasum) found"
    fi
    [ "$got" = "$want" ] || die "sha256 mismatch for $(basename "$file"): got $got want $want"
}
verify "$TMP/$ARCHIVE" "$TMP/$ARCHIVE.sha256"
verify "$TMP/$PAYLOAD" "$TMP/$PAYLOAD.sha256"
say "nest-install: checksums verified"

mkdir -p "$BIN_DIR" "$DATA_DIR"
tar -xJf "$TMP/$ARCHIVE" -C "$TMP"
cp "$TMP/nest-cli-$TARGET/nest" "$BIN_DIR/nest"
chmod +x "$BIN_DIR/nest"
tar -xzf "$TMP/$PAYLOAD" -C "$DATA_DIR"

say "nest-install: installed $BIN_DIR/nest"
say "nest-install: embedder payload at $PAYLOAD_DIR"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "nest-install: note: $BIN_DIR is not on your PATH" ;;
esac
say "nest-install: run \`nest doctor\` to validate the install (offline)"
