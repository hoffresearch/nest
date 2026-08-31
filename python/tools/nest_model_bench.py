"""nest_model_bench.py — three-tier model comparison over a multi-space .nest
(RFC-5). Tiers are measured and reported SEPARATELY, never aggregated:

  T1 pipeline stability : identity self-retrieval@k with fresh SOURCE-image
      queries + embedding drift (cosine source-embed vs the decoded-media
      vectors the index serves). Inflated by construction; says the pipeline
      preserves its own signal, not that the model is good.
  T2 codec cost         : drift statistics per model (the full two-index
      source-vs-decoded retrieval delta lives in the sweep; this reports the
      per-item drift the single decoded index allows honestly).
  T3 task utility       : text->image hit@k via the label ruler (weak labels,
      declared) and, when --queries-file is given, real operator queries with
      expected/negative ids (hit@k, MRR, negative leakage).

Usage:
  nest_model_bench.py --index out/spellbook/spellbook.nest -k 1 5 10
      [--queries 100] [--seed 42] [--text-query-template "artwork of {label}"]
      [--queries-file q.json] [--out bench.json]
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "python"))

import nest
import numpy as np
from forge import model_registry


def load_sidecars(index: Path) -> dict:
    manifest = json.loads(index.with_suffix(".manifest.json").read_text())
    if manifest.get("manifest_schema_version") != 1:
        raise SystemExit(
            f"unsupported manifest_schema_version: {manifest.get('manifest_schema_version')}"
        )
    return manifest


def image_spaces(manifest: dict) -> dict[str, list[dict]]:
    """preset -> its image-space records (may be several dims)."""
    out: dict[str, list[dict]] = {}
    for s in manifest["spaces"]:
        if s["modality"] == "image":
            out.setdefault(s["preset"], []).append(s)
    return out


def make_adapter(manifest: dict, preset: str):
    meta = manifest["models"][preset]
    recipe = meta.get("recipe", {})
    return model_registry.create_embedder(
        preset,
        allow_remote_code=frozenset(manifest["models"].keys()),
        allow_heavy=True,
        usage=recipe,
        batch_size=8,
    )


def pick_items(manifest: dict, n: int, seed: int) -> list[dict]:
    items = [it for it in manifest["items"] if it.get("image_path")]
    rng = np.random.default_rng(seed)
    idx = sorted(rng.choice(len(items), size=min(n, len(items)), replace=False).tolist())
    return [items[i] for i in idx]


def _expand(p: str) -> Path:
    return Path(p.replace("~", str(Path.home()), 1)) if p.startswith("~") else Path(p)


def bench_model(
    db,
    manifest: dict,
    preset: str,
    spaces: list[dict],
    items: list[dict],
    ks: list[int],
    template: str,
) -> dict:
    adapter = make_adapter(manifest, preset)
    paths = [_expand(it["image_path"]) for it in items]
    t0 = time.time()
    src_vecs = adapter.embed_paths(paths)
    embed_s = time.time() - t0

    report: dict = {
        "embed_side_items_per_s": round(len(items) / max(embed_s, 1e-9), 2),
        "build_items_per_s": manifest["models"][preset].get("items_per_s"),
        "spaces": {},
    }
    for space in spaces:
        name, dim = space["name"], space["dim"]
        q = model_registry.slice_renorm(src_vecs, dim) if dim else src_vecs
        # T1: identity self-retrieval with source queries through the codec
        identity = {k: 0 for k in ks}
        drifts = []
        lat = []
        for i, it in enumerate(items):
            t0 = time.time()
            hits = db.search_space(name, q[i].tolist(), max(ks))
            lat.append((time.time() - t0) * 1e3)
            ranked = [h.offset_start for h in hits]
            for k in ks:
                identity[k] += int(it["ordinal"] in ranked[:k])
            own = next((h.score for h in hits if h.offset_start == it["ordinal"]), None)
            if own is not None:
                drifts.append(own)  # cosine(source-embed, stored decoded vector)
        n = len(items)
        space_rep = {
            "t1_identity_recall": {f"@{k}": round(identity[k] / n, 4) for k in ks},
            "t2_drift_cosine": {
                "p10": round(float(np.percentile(drifts, 10)), 4) if drifts else None,
                "p50": round(float(np.percentile(drifts, 50)), 4) if drifts else None,
                "n_found": len(drifts),
            },
            "latency_ms": {
                "p50": round(float(np.percentile(lat, 50)), 3),
                "p95": round(float(np.percentile(lat, 95)), 3),
            },
            "band_bytes": space.get("band_bytes"),
        }
        # T3: text->image name ruler (weak labels, declared as such)
        labels = [it.get("label") for it in items]
        if template and all(labels) and "text" in model_registry.get_preset(preset).modalities:
            tq = adapter.embed_texts([template.format(label=lb) for lb in labels], role="query")
            tq = model_registry.slice_renorm(tq, dim) if dim else tq
            t3 = {k: 0 for k in ks}
            for i, it in enumerate(items):
                ranked = [h.offset_start for h in db.search_space(name, tq[i].tolist(), max(ks))]
                for k in ks:
                    t3[k] += int(it["ordinal"] in ranked[:k])
            space_rep["t3_text_to_image_hit"] = {f"@{k}": round(t3[k] / n, 4) for k in ks}
            space_rep["t3_ruler"] = "label-template (weak ground truth)"
        report["spaces"][name] = space_rep
    return report


def bench_queries_file(db, manifest: dict, qfile: Path, ks: list[int]) -> dict:
    """Real operator queries: [{query, expected_keys[], negative_keys[]?}]."""
    queries = json.loads(qfile.read_text())
    by_key = {it["key"]: it["ordinal"] for it in manifest["items"]}
    out: dict[str, dict] = {}
    for s in manifest["spaces"]:
        if s["modality"] != "image":
            continue
        preset = s["preset"]
        if "text" not in model_registry.get_preset(preset).modalities:
            continue
        adapter = make_adapter(manifest, preset)
        hitk = {k: 0.0 for k in ks}
        mrr, leak, n = 0.0, 0.0, 0
        for q in queries:
            expected = {by_key[x] for x in q["expected_keys"] if x in by_key}
            negative = {by_key[x] for x in q.get("negative_keys", []) if x in by_key}
            if not expected:
                continue
            v = adapter.embed_texts([q["query"]], role="query")
            v = model_registry.slice_renorm(v, s["dim"])[0] if s["dim"] else v[0]
            ranked = [h.offset_start for h in db.search_space(s["name"], v.tolist(), max(ks))]
            n += 1
            for k in ks:
                hitk[k] += int(bool(expected & set(ranked[:k])))
            rank = next((i + 1 for i, o in enumerate(ranked) if o in expected), None)
            mrr += (1.0 / rank) if rank else 0.0
            if negative:
                leak += len(negative & set(ranked[: max(ks)])) / len(negative)
        if n:
            out[s["name"]] = {
                "n_queries": n,
                "hit": {f"@{k}": round(hitk[k] / n, 4) for k in ks},
                "mrr": round(mrr / n, 4),
                "negative_leakage": round(leak / n, 4),
            }
    return out


def print_table(report: dict, ks: list[int]) -> None:
    print("\n== T1 pipeline stability (identity@k, inflated by construction) ==")
    for _preset, rep in report["models"].items():
        for name, sp in rep["spaces"].items():
            print(
                f"  {name:<24} "
                + "  ".join(f"id@{k}={sp['t1_identity_recall'][f'@{k}']:.3f}" for k in ks)
            )
    print("\n== T2 codec cost (drift cosine: source-embed vs stored decoded) ==")
    for _preset, rep in report["models"].items():
        for name, sp in rep["spaces"].items():
            d = sp["t2_drift_cosine"]
            print(f"  {name:<24} p10={d['p10']}  p50={d['p50']}")
    print("\n== T3 task utility (never compare against T1/T2 numbers) ==")
    for _preset, rep in report["models"].items():
        for name, sp in rep["spaces"].items():
            if "t3_text_to_image_hit" in sp:
                print(
                    f"  {name:<24} "
                    + "  ".join(f"txt@{k}={sp['t3_text_to_image_hit'][f'@{k}']:.3f}" for k in ks)
                    + f"  [{sp['t3_ruler']}]"
                )
    print("\n== cost ==")
    for _preset, rep in report["models"].items():
        for name, sp in rep["spaces"].items():
            mb = (sp.get("band_bytes") or 0) / 1e6
            print(
                f"  {name:<24} {mb:7.2f} MB  lat p50={sp['latency_ms']['p50']}ms "
                f"p95={sp['latency_ms']['p95']}ms  embed={rep['embed_side_items_per_s']} it/s"
            )
    if report.get("operator_queries"):
        print("\n== T3 operator queries ==")
        for name, r in report["operator_queries"].items():
            print(
                f"  {name:<24} "
                + "  ".join(f"hit@{k}={r['hit'][f'@{k}']}" for k in ks)
                + f"  mrr={r['mrr']} leak={r['negative_leakage']}"
            )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--index", required=True, type=Path)
    ap.add_argument("-k", type=int, nargs="+", default=[1, 5, 10])
    ap.add_argument("--queries", type=int, default=100)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--text-query-template", default="")
    ap.add_argument("--queries-file", type=Path)
    ap.add_argument("--models", help="comma-separated preset subset")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()

    manifest = load_sidecars(args.index)
    db = nest.open(str(args.index))
    # band geometry comes from the runtime's inspect (the manifest sidecar
    # does not duplicate it): fold band_bytes into the space records.
    import subprocess

    cli = REPO / "target" / "release" / "nest"
    if cli.exists():
        doc = json.loads(
            subprocess.run(
                [str(cli), "inspect", "--json", str(args.index)], capture_output=True, text=True
            ).stdout
        )
        bands = {sp["name"]: sp.get("band_bytes") for sp in (doc.get("spaces") or [])}
        for sp in manifest["spaces"]:
            sp["band_bytes"] = bands.get(sp["name"])
    spaces = image_spaces(manifest)
    if args.models:
        spaces = {p: s for p, s in spaces.items() if p in args.models.split(",")}
    items = pick_items(manifest, args.queries, args.seed)

    report = {
        "bench_schema_version": 1,
        "index": str(args.index),
        "n_query_items": len(items),
        "ks": args.k,
        "tiers_note": "T1/T2/T3 answer different questions and are never aggregated",
        "models": {},
    }
    for preset, sps in spaces.items():
        print(f"[bench] {preset} ({len(sps)} space(s))...", flush=True)
        report["models"][preset] = bench_model(
            db, manifest, preset, sps, items, args.k, args.text_query_template
        )
    if args.queries_file:
        report["operator_queries"] = bench_queries_file(db, manifest, args.queries_file, args.k)

    print_table(report, args.k)
    if args.out:
        args.out.write_text(json.dumps(report, indent=1))
        print(f"\nwritten: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
