"""console entry point shipped by the `nestdb` wheel (issue #75).

`uvx --from nestdb nest ...` / `pipx run` get a working `nest` command from
the python package alone. this is a THIN read-only shim over the library
api: validate / inspect / stats / search. the full verb set (ask, retrieve,
search-text, benchmark, cite, doctor) lives in the rust `nest` binary,
installed by scripts/install.sh.

dev repo usage:  python3 python/nest_cli.py validate dat/corpus_next.v1.nest
installed usage: nest validate corpus.nest
"""

from __future__ import annotations

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # dev layout
import nest  # noqa: E402


def _cmd_validate(args) -> int:
    db = nest.open(args.file)
    db.validate()
    print(f"ok: {args.file}")
    print(f"  file_hash:    {db.file_hash}")
    print(f"  content_hash: {db.content_hash}")
    return 0


def _cmd_inspect(args) -> int:
    db = nest.open(args.file)
    print(json.dumps(db.inspect(), indent=2, default=str))
    return 0


def _cmd_stats(args) -> int:
    db = nest.open(args.file)
    print(f"file:           {args.file}")
    print(f"embedding_dim:  {db.embedding_dim}")
    print(f"n_embeddings:   {db.n_embeddings}")
    print(f"dtype:          {db.dtype}")
    print(f"simd_backend:   {db.simd_backend}")
    print(f"has_ann:        {db.has_ann}")
    print(f"has_bm25:       {db.has_bm25}")
    print(f"has_graph:      {db.has_graph}")
    print(f"model_hash:     {db.model_hash}")
    print(f"file_hash:      {db.file_hash}")
    print(f"content_hash:   {db.content_hash}")
    return 0


def _cmd_search(args) -> int:
    qvec = json.loads(args.query)
    db = nest.open(args.file)
    hits = db.search(qvec, args.k)
    for i, h in enumerate(hits):
        print(f"[{i + 1}] score={h.score:.6f} chunk_id={h.chunk_id} citation={h.citation_id}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(
        prog="nest",
        description="read-only nest verbs from the nestdb wheel; "
        "the full cli is the rust binary (scripts/install.sh)",
    )
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("validate", help="full integrity check")
    p.add_argument("file")
    p.set_defaults(fn=_cmd_validate)

    p = sub.add_parser("inspect", help="header, manifest, hashes as json")
    p.add_argument("file")
    p.set_defaults(fn=_cmd_inspect)

    p = sub.add_parser("stats", help="sizes, dtype, simd backend, hashes")
    p.add_argument("file")
    p.set_defaults(fn=_cmd_stats)

    p = sub.add_parser("search", help="exact top-k; query is a json array of f32")
    p.add_argument("file")
    p.add_argument("query")
    p.add_argument("-k", type=int, default=10)
    p.set_defaults(fn=_cmd_search)

    args = ap.parse_args()
    try:
        return args.fn(args)
    except Exception as e:  # the pyo3 layer raises typed errors; print + exit
        print(f"nest: error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
