"""stream documents out of a (possibly huge) zip of json entries WITHOUT
extracting it.

built for the real case where a researcher holds a giant packed archive and has
no room (or no time) to extract it: forge ingests straight from the zip, one
entry at a time, so resident memory stays O(a single entry) no matter how large
the archive is. when the archive is stored (not deflated) each entry is a cheap
seek + read, so streaming the whole corpus costs one linear pass and never
materializes the uncompressed bytes on disk.

generic by design: nothing dataset-specific is hardcoded. point it at any zip,
give it an entry glob and the json field paths that carry the text (and any
metadata fields to carry through for a downstream recall harness). a stable,
citable source_uri is derived per entry so a built .nest can cite straight back
into the archive.
"""

from __future__ import annotations

import fnmatch
import json
import os
import zipfile
from collections.abc import Iterator, Sequence
from dataclasses import dataclass


@dataclass(frozen=True)
class ZipDoc:
    """one streamed document: the joined text to index, a stable source_uri,
    and any pass-through metadata (e.g. labels for a recall harness)."""

    source_uri: str
    text: str
    meta: dict


def _dig(obj: object, dotted: str) -> object:
    """resolve a dotted json path (``a.b.c``) or return None if absent."""
    cur = obj
    for part in dotted.split("."):
        if not isinstance(cur, dict):
            return None
        cur = cur.get(part)
    return cur


def entry_names(
    zip_path: str, *, name_glob: str = "*.json", stride: int = 1, limit: int | None = None
) -> list[str]:
    """the deterministic, sorted list of archive entries to stream.

    sorting makes the build order (and therefore the .nest bytes) reproducible.
    `stride` takes every Nth entry (a spread, deterministic sample of a huge
    archive); `limit` caps the count after striding.
    """
    with zipfile.ZipFile(zip_path) as z:
        names = [
            n
            for n in z.namelist()
            if not n.endswith("/") and fnmatch.fnmatch(os.path.basename(n), name_glob)
        ]
    names.sort()
    if stride > 1:
        names = names[::stride]
    if limit is not None:
        names = names[:limit]
    return names


def stream_zip_json(
    zip_path: str,
    *,
    text_fields: Sequence[str],
    name_glob: str = "*.json",
    meta_fields: Sequence[str] = (),
    text_join: str = "\n\n",
    stride: int = 1,
    limit: int | None = None,
    names: Sequence[str] | None = None,
    source_scheme: str | None = None,
) -> Iterator[ZipDoc]:
    """yield one :class:`ZipDoc` per matching entry, reading the archive lazily.

    only one entry is held in memory at a time. `text_fields` are dotted json
    paths whose string values are joined into the indexable text; `meta_fields`
    are carried through untouched (labels, ids) for a recall harness. entries
    whose text is empty after the join are skipped (they cannot be embedded).
    pass an explicit `names` list (e.g. a selected cohort) to stream exactly
    those entries in order, bypassing the glob/stride/limit sampling.
    """
    zname = os.path.basename(zip_path)
    scheme = source_scheme or f"zip://{zname}"
    if names is None:
        names = entry_names(zip_path, name_glob=name_glob, stride=stride, limit=limit)
    with zipfile.ZipFile(zip_path) as z:
        for n in names:
            try:
                obj = json.loads(z.read(n))
            except (json.JSONDecodeError, KeyError, zipfile.BadZipFile):
                continue
            parts = [
                v.strip()
                for fld in text_fields
                if isinstance((v := _dig(obj, fld)), str) and v.strip()
            ]
            text = text_join.join(parts)
            if not text.strip():
                continue
            meta = {m: _dig(obj, m) for m in meta_fields}
            yield ZipDoc(source_uri=f"{scheme}#{n}", text=text, meta=meta)
