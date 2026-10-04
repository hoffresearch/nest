#!/bin/sh
# fetch the vendored potion table without git-lfs. the release and wheel
# jobs used to `git lfs pull`, which spends the repo's lfs budget on every
# build and stops the release when that budget runs out. the table is the
# upstream file byte for byte, so this downloads it from hugging face at the
# pinned revision and accepts it only when its sha256 is the oid recorded in
# the lfs pointer that git checked out. a real (already smudged) file is
# left alone.
set -eu

FILE="python/forge/models/potion-base-8M/model.safetensors"
REV="bf8b056651a2c21b8d2565580b8569da283cab23"
URL="https://huggingface.co/minishlab/potion-base-8M/resolve/$REV/model.safetensors"

size=$(wc -c <"$FILE" | tr -d ' ')
if [ "$size" -gt 1024 ]; then
    echo "getpotion: $FILE is already the table ($size bytes)"
    exit 0
fi
oid=$(sed -n 's/^oid sha256://p' "$FILE")
[ -n "$oid" ] || { echo "getpotion: $FILE is neither the table nor an lfs pointer" >&2; exit 1; }

curl -fsSL --retry 3 -o "$FILE.part" "$URL"
got=$( (sha256sum "$FILE.part" 2>/dev/null || shasum -a 256 "$FILE.part") | cut -d' ' -f1)
if [ "$got" != "$oid" ]; then
    rm -f "$FILE.part"
    echo "getpotion: sha256 $got does not match the pointer oid $oid" >&2
    exit 1
fi
mv "$FILE.part" "$FILE"
echo "getpotion: $FILE sha256:$got"
