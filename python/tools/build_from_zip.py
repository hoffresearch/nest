"""build a .nest by STREAMING documents straight out of a giant zip
(see python/forge/zip_source.py) without extracting the archive.

ingestion holds one archive entry at a time and embeddings are computed in
fixed-size batches, so peak memory stays flat regardless of archive size. a
sidecar jsonl records per-document labels (the metadata fields you ask for) so a
real-query recall harness can score retrieval against the data's own structure.
all paths are arguments: no dataset, and no sensitive path, is hardcoded. for
sensitive corpora keep --out and --sidecar OUTSIDE any git working tree.

scale note: nest.build() currently assembles the chunk list in memory, so this
proves the STREAMING INGESTION path end to end and is bounded by the chunk count
(not the archive size). a fully incremental writer that streams straight into
the .nest via the .fci intermediate is the next step for corpora whose chunk set
exceeds memory.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))  # python/
import nest  # noqa: E402
from builder import chunk_text  # noqa: E402
from forge.embed_potion import potion_embedder  # noqa: E402
from forge.zip_source import stream_zip_json  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser(description="stream a zip of json docs into a .nest")
    ap.add_argument("--zip", required=True)
    ap.add_argument("--out", required=True, help="output .nest path (keep OUT of any git repo for PHI)")
    ap.add_argument("--sidecar", required=True, help="output jsonl of per-doc labels (also keep out of git)")
    ap.add_argument("--name-glob", default="*.json")
    ap.add_argument("--text-fields", required=True, help="comma-sep dotted json paths to INDEX")
    ap.add_argument("--meta-fields", default="", help="comma-sep dotted json paths to carry as labels")
    ap.add_argument(
        "--index-fields",
        default="",
        help="comma-sep dotted json paths to ALSO emit as a per-chunk meta_index (0x17), "
        "so search_filtered can scope the exact cosine to a (field,value). generic: any label.",
    )
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--stride", type=int, default=1)
    ap.add_argument("--names-file", default=None, help="file of explicit entry names (a cohort); overrides glob/stride/limit")
    ap.add_argument("--query-chars", type=int, default=512, help="head of the body stored in the sidecar as the body-query")
    ap.add_argument("--max-chars", type=int, default=512, help="chunk window")
    ap.add_argument("--max-doc-chars", type=int, default=20000, help="truncate any one doc to this (caps giant pastes)")
    ap.add_argument("--batch", type=int, default=1024, help="embedding batch size (memory knob)")
    ap.add_argument("--preset", default="exact")
    ap.add_argument("--dtype", default=None, help="override: float32|float16|int8|int4")
    ap.add_argument("--text-encoding", default=None, help="override: raw|zstd")
    ap.add_argument("--mrl-dim", type=int, default=None, help="matryoshka prefix dim (truncate+renorm)")
    ap.add_argument("--with-hnsw", action="store_true", help="force the ann index on")
    ap.add_argument("--model-dir", default=None, help="potion/model2vec table dir (default: vendored potion-base-8M)")
    ap.add_argument(
        "--dedup",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="drop exact-duplicate chunks (same normalized text) and empty chunks "
        "before indexing. on by default: identical vectors are pure retrieval noise "
        "(measured ~7.7%% of chunks on the clinical cohort, one block repeated 240x). "
        "use --no-dedup to keep every chunk.",
    )
    args = ap.parse_args()

    text_fields = [f.strip() for f in args.text_fields.split(",") if f.strip()]
    meta_fields = [f.strip() for f in args.meta_fields.split(",") if f.strip()]
    index_fields = [f.strip() for f in args.index_fields.split(",") if f.strip()]
    # one value per CHUNK (a doc's label repeated across its chunks); assembled
    # into the meta_index and handed to nest.build. flat memory: same O(n_chunks)
    # as the chunk list, and labels repeat heavily so it stays small.
    meta_cols: dict[str, list] = {f: [] for f in index_fields}

    emb = potion_embedder(args.model_dir) if args.model_dir else potion_embedder()
    dim = emb.embedding_dim  # touches the table; fails loud if not present

    chunks: list[dict] = []
    pending_texts: list[str] = []
    pending_idx: list[int] = []
    n_docs = n_truncated = 0
    seen_chunks: set[bytes] = set()  # normalized-text hashes, for --dedup
    n_dup_dropped = n_empty_dropped = 0
    t0 = time.time()

    def flush() -> None:
        if not pending_texts:
            return
        for i, v in zip(pending_idx, emb.embed_texts(pending_texts), strict=True):
            chunks[i]["embedding"] = v
        pending_texts.clear()
        pending_idx.clear()

    cohort = None
    if args.names_file:
        with open(args.names_file) as nf:
            cohort = [ln.strip() for ln in nf if ln.strip()]

    with open(args.sidecar, "w") as sc:
        for doc in stream_zip_json(
            args.zip,
            text_fields=text_fields,
            name_glob=args.name_glob,
            meta_fields=meta_fields,
            stride=args.stride,
            limit=args.limit,
            names=cohort,
        ):
            text = doc.text
            if len(text) > args.max_doc_chars:
                text = text[: args.max_doc_chars]
                n_truncated += 1
            specs = chunk_text(text, doc.source_uri, max_chars=args.max_chars)
            kept = 0
            for s in specs:
                if args.dedup:
                    if not s.canonical_text.strip():
                        n_empty_dropped += 1
                        continue
                    key = hashlib.blake2b(
                        " ".join(s.canonical_text.split()).encode("utf-8"),
                        digest_size=16,
                    ).digest()
                    if key in seen_chunks:
                        n_dup_dropped += 1
                        continue
                    seen_chunks.add(key)
                chunks.append(
                    {
                        "canonical_text": s.canonical_text,
                        "source_uri": s.source_uri,
                        "byte_start": s.byte_start,
                        "byte_end": s.byte_end,
                        "embedding": None,
                    }
                )
                pending_texts.append(s.canonical_text)
                pending_idx.append(len(chunks) - 1)
                kept += 1
                if len(pending_texts) >= args.batch:
                    flush()
            # one meta value per KEPT chunk, so meta_cols stays aligned with the
            # (possibly deduped) chunk list. a globally-duplicate chunk is indexed
            # once, under the FIRST doc that produced it.
            for f in index_fields:
                v = doc.meta.get(f)
                meta_cols[f].extend([None if v is None else str(v)] * kept)
            sc.write(
                json.dumps(
                    {
                        "source_uri": doc.source_uri,
                        "n_chunks": kept,
                        "query_text": text[: args.query_chars],
                        "meta": doc.meta,
                    },
                    ensure_ascii=False,
                )
                + "\n"
            )
            n_docs += 1
            if n_docs % 2000 == 0:
                print(
                    f"  streamed {n_docs} docs, {len(chunks)} chunks, {time.time() - t0:.0f}s",
                    file=sys.stderr,
                )
        flush()

    if args.dedup:
        dropped = n_dup_dropped + n_empty_dropped
        denom = len(chunks) + dropped
        pct = (100.0 * dropped / denom) if denom else 0.0
        print(
            f"dedup: dropped {n_dup_dropped} duplicate + {n_empty_dropped} empty "
            f"chunks ({pct:.1f}% of {denom}); {len(chunks)} unique chunks indexed.",
            file=sys.stderr,
        )
    print(
        f"streamed {n_docs} docs ({n_truncated} truncated) -> {len(chunks)} chunks "
        f"in {time.time() - t0:.0f}s; building .nest preset={args.preset}...",
        file=sys.stderr,
    )
    if os.path.exists(args.out):
        os.unlink(args.out)
    nest.build(
        output_path=args.out,
        embedding_model=emb.embedding_model,
        embedding_dim=dim,
        chunker_version="char-v1",
        model_hash=emb.model_hash(),
        chunks=chunks,
        title="streamed-from-zip",
        reproducible=True,
        preset=args.preset,
        dtype=args.dtype,
        text_encoding=args.text_encoding,
        mrl_dim=args.mrl_dim,
        meta_index=(meta_cols if index_fields else None),
        with_hnsw=(True if args.with_hnsw else None),
    )
    db = nest.open(args.out)
    db.validate()
    print(
        f"built {args.out}: {db.n_embeddings} embeddings, dtype={db.dtype}, dim={db.embedding_dim}",
        file=sys.stderr,
    )
    print(
        json.dumps(
            {
                "out": args.out,
                "sidecar": args.sidecar,
                "n_docs": n_docs,
                "n_truncated": n_truncated,
                "n_chunks": len(chunks),
                "preset": args.preset,
                "dim": dim,
                "model": emb.embedding_model,
            }
        )
    )


if __name__ == "__main__":
    main()
