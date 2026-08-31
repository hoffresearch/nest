"""Declarative build orchestration (RFC-1): rows → dedup → media → embed → emit.

Transactional (RFC-0 N7): each expensive stage records completion + a params
hash under `<out>/.forge-state/`; outputs are written to `<out>/.tmp/` and
committed by atomic rename; `resume=True` skips stages whose params match and
whose artifacts verify. Embedding caches are their own state (triad-keyed,
forge_cache). Rows are cheap and always recomputed — their corpus_input_hash
is what the other stages key on.

Sharing (N4): per-model outputs reuse the one media encode, blob table and
cached vectors; space 0 of every file is the one text="default" model (N14).
"""

from __future__ import annotations

import json
import os
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import numpy as np

from forge import image_media, model_registry
from forge.build_spec import CorpusSpec, ModelSpec, default_model, emitted_spaces, validate
from forge.corpus_sources import Row, corpus_input_hash, load_rows
from forge.forge_cache import EmbedCache, atomic_write_json, canonical_hash
from forge.forge_manifest import build_lock, check_lock, redact_path, write_manifest

ADAPTER_VERSION = 1


class ForgeError(RuntimeError):
    pass


@dataclass
class _Ctx:
    spec: CorpusSpec
    out_dir: Path
    rows: list[Row] = field(default_factory=list)
    unique: list[Row] = field(default_factory=list)  # dedup: first occurrence per image hash
    frame_of_row: list[int] = field(default_factory=list)
    input_hash: str = ""
    media: dict | None = None
    frame_uris: list[str] = field(default_factory=list)  # per UNIQUE frame, item order
    vectors: dict[str, dict[str, np.ndarray]] = field(default_factory=dict)  # preset -> arrays
    model_meta: dict[str, dict] = field(default_factory=dict)
    timings: dict[str, float] = field(default_factory=dict)

    @property
    def state_dir(self) -> Path:
        return self.out_dir / ".forge-state"

    @property
    def tmp_dir(self) -> Path:
        return self.out_dir / ".tmp"


def build(
    spec: CorpusSpec,
    *,
    sample: int | None = None,
    seed: int = 42,
    models_filter: list[str] | None = None,
    resume: bool = False,
    rebuild_only: bool = False,
    strict_env: bool = False,
    allow_heavy: bool = False,
) -> dict:
    validate(spec, allow_heavy=allow_heavy)
    if models_filter:
        spec.models = [m for m in spec.models if m.preset in models_filter]
        validate(spec, allow_heavy=allow_heavy)
    ctx = _Ctx(spec=spec, out_dir=Path(spec.output.dir))
    ctx.out_dir.mkdir(parents=True, exist_ok=True)
    ctx.state_dir.mkdir(exist_ok=True)
    ctx.tmp_dir.mkdir(exist_ok=True)

    t0 = time.time()
    ctx.rows = load_rows(spec, sample=sample, seed=seed)
    if not ctx.rows:
        raise ForgeError("source produced no rows")
    ctx.input_hash = corpus_input_hash(ctx.rows)
    _dedup(ctx)
    ctx.timings["rows"] = round(time.time() - t0, 3)

    _media_stage(ctx, resume=resume or rebuild_only)
    _embed_stage(ctx, rebuild_only=rebuild_only)
    from forge import forge_emit

    result = forge_emit._emit(ctx)
    forge_emit._finalize(ctx, result, strict_env=strict_env, rebuild_only=rebuild_only)
    return result


def _dedup(ctx: _Ctx) -> None:
    dedup_on = ctx.spec.media.dedup if ctx.spec.media else True
    seen: dict[str, int] = {}
    for row in ctx.rows:
        key = row.image_sha256 if (row.image_sha256 and dedup_on) else f"row:{row.ordinal}"
        if key not in seen:
            seen[key] = len(ctx.unique)
            ctx.unique.append(row)
        ctx.frame_of_row.append(seen[key])


def _media_params(ctx: _Ctx) -> str:
    return canonical_hash({"input": ctx.input_hash, "media": asdict(ctx.spec.media)})


def _media_stage(ctx: _Ctx, *, resume: bool) -> None:
    spec = ctx.spec
    if spec.media is None or not any(r.image_path for r in ctx.unique):
        return
    missing = [r.key for r in ctx.rows if r.image_path is None]
    if missing:
        raise ForgeError(
            f"media enabled but {len(missing)} rows have no image (first: {missing[0]})"
        )

    state_file = ctx.state_dir / "media.json"
    params = _media_params(ctx)
    media_dir = image_media.media_dir_for(ctx.out_dir / f"{spec.name}.nest")
    if resume and state_file.is_file():
        st = json.loads(state_file.read_text())
        if st.get("params") == params and _media_files_ok(media_dir, st["media"]):
            ctx.media, ctx.frame_uris = st["media"], st["frame_uris"]
            ctx.timings["media"] = 0.0
            return
        if resume and st.get("params") != params:
            pass  # spec/input changed: re-encode
    t0 = time.time()
    ctx.media, ctx.frame_uris = _encode_media(ctx, media_dir)
    ctx.timings["media"] = round(time.time() - t0, 3)
    atomic_write_json(
        state_file,
        {"params": params, "media": ctx.media, "frame_uris": ctx.frame_uris, "done": True},
    )


def _media_files_ok(media_dir: Path, media: dict) -> bool:
    for seg in media.get("segments", [{"uri": None}]):
        if seg["uri"] is None:  # unsharded record
            return any(media_dir.glob("*"))
        p = media_dir / seg["uri"]
        if not p.is_file():
            return False
        if seg.get("media_sha256") and image_media.sha256_file(p) != seg["media_sha256"]:
            return False
    return True


def _gate_adapter(ctx: _Ctx, preset_name: str):
    ms = next(m for m in ctx.spec.models if m.preset == preset_name)
    return model_registry.create_embedder(
        preset_name,
        model_path=ms.model_path or None,
        device=ms.device or None,
        batch_size=ms.batch_size,
        allow_remote_code=frozenset(ctx.spec.output.allow_remote_code),
        allow_heavy=True,
    )


def _encode_media(ctx: _Ctx, media_dir: Path) -> tuple[dict, list[str]]:
    from forge import image_backends

    m = ctx.spec.media
    paths = [r.image_path for r in ctx.unique]
    canvas = image_media.canvas_size(paths, m.width)

    crf = m.crf
    quality_report = None
    if crf == "auto":
        from forge import quality_gate

        gate = _gate_adapter(ctx, m.quality.gate_model or _first_image_preset(ctx.spec))
        crf, quality_report = quality_gate.choose_crf(paths, canvas, m, gate)

    order = None
    if m.order in ("similarity", "cluster"):
        from forge import image_order

        gate = _gate_adapter(ctx, m.cluster.space or _first_image_preset(ctx.spec))
        vecs = gate.embed_paths(paths)
        order = (
            image_order.similarity_order(vecs)
            if m.order == "similarity"
            else image_order.cluster_order(vecs, m.cluster.threshold)
        )

    built = image_backends.build_media(
        paths,
        ctx.out_dir / f"{ctx.spec.name}.nest",
        ctx.spec.name,
        backend=m.backend,
        canvas=canvas,
        crf=int(crf),
        speed=m.speed,
        all_intra=(m.gop == "intra"),
        pix_fmt=m.pix_fmt,
        avif_quality=int(crf) if isinstance(crf, int) else 35,
        control=(m.backend == "control"),
        gop_policy=m.gop if m.gop != "intra" else "intra",
        order=order,
        shard_size=m.shard_size,
        tune=m.tune,
        fps=m.fps,
        jxl_transcode=m.jxl_transcode,
    )
    media = built["media"]
    media["dedup"] = {"n_items": len(ctx.rows), "n_unique_frames": len(ctx.unique)}
    if quality_report is not None:
        media["crf_auto"] = quality_report
    return media, built["uris"]


def _first_image_preset(spec: CorpusSpec) -> str:
    return next(m.preset for m in spec.models if m.image == "space")


def _recipe(ctx: _Ctx, ms: ModelSpec, preset) -> dict:
    mode = ctx.spec.image_input_mode()
    recipe = {
        "adapter_version": ADAPTER_VERSION,
        "preset": ms.preset,
        "image_input_mode": mode,
        "text_corpus_mode": ms.text_corpus_mode or preset.text_corpus_mode,
        "text_query_mode": ms.text_query_mode or preset.text_query_mode,
        "image_mode": ms.image_mode or preset.image_mode,
        "image_prompt": ms.image_prompt or preset.image_prompt,
        "normalize": ms.normalize,
        "preprocess_version": ms.preprocess_version or preset.preprocess_version,
        "image_max_side": ms.image_max_side or preset.image_max_side,
        "image_doc_format": getattr(preset, "image_doc_format", "dict"),
        "encode_kwargs": ms.encode_kwargs,
        "model_dtype": ms.dtype,
        "device_class": ms.device or "auto",
    }
    if mode == "decoded_media" and ctx.media is not None:
        recipe["decoder"] = {
            "backend": ctx.media.get("backend"),
            "canvas": ctx.media.get("canvas"),
            "crf": ctx.media.get("crf"),
            "pix_fmt": ctx.media.get("pix_fmt"),
            "provenance_sha256": ctx.media.get("provenance_sha256"),
        }
    return recipe


def _embed_stage(ctx: _Ctx, *, rebuild_only: bool) -> None:
    spec = ctx.spec
    for ms in spec.models:
        preset = model_registry.get_preset(ms.preset)
        cache = EmbedCache(ctx.out_dir / ".cache", spec.name, ms.preset)
        adapter = None
        recipe = _recipe(ctx, ms, preset)
        recipe_hash = canonical_hash(recipe)

        def get_adapter(ms=ms, recipe=recipe):
            nonlocal adapter
            if adapter is None:
                adapter = model_registry.create_embedder(
                    ms.preset,
                    model_path=ms.model_path or None,
                    device=ms.device or None,
                    batch_size=ms.batch_size,
                    allow_remote_code=frozenset(spec.output.allow_remote_code),
                    allow_heavy=True,
                    usage=recipe,
                )
            return adapter

        # the triad needs model_hash, which needs a loaded model; cache a probe of it
        probe_file = ctx.state_dir / f"model_hash.{ms.preset}.json"
        if probe_file.is_file():
            model_hash = json.loads(probe_file.read_text())["model_hash"]
        else:
            model_hash = get_adapter().model_hash
            atomic_write_json(probe_file, {"model_hash": model_hash})
        triad = {
            "model_hash": model_hash,
            "embedding_recipe_hash": recipe_hash,
            "corpus_input_hash": ctx.input_hash,
        }

        arrays = cache.load(triad)
        if arrays is None:
            if rebuild_only:
                raise ForgeError(
                    f"--rebuild-only: cache for '{ms.preset}' is missing or stale "
                    "(triad mismatch); run a full build"
                )
            t0 = time.time()
            arrays = {}
            if ms.text in ("default", "space"):
                arrays["text"] = get_adapter().embed_texts([r.canonical_text for r in ctx.rows])
            if ms.image == "space":
                arrays["image_unique"] = _embed_images(ctx, get_adapter())
            elapsed = max(time.time() - t0, 1e-9)
            n = sum(a.shape[0] for a in arrays.values())
            ctx.timings[f"embed.{ms.preset}"] = round(elapsed, 3)
            ctx.model_meta.setdefault(ms.preset, {})["items_per_s"] = round(n / elapsed, 2)
            cache.store(triad, arrays)
        else:
            ctx.timings[f"embed.{ms.preset}"] = 0.0
        # model_hash was probed without loading when cached; verify on real loads only
        if adapter is not None and adapter.model_hash != model_hash:
            raise ForgeError(f"model_hash drift for '{ms.preset}': stale .forge-state probe")
        if adapter is not None and hasattr(adapter, "close"):
            adapter.close()  # st workers: return the model's memory before the next model
        ctx.vectors[ms.preset] = arrays
        ctx.model_meta.setdefault(ms.preset, {}).update(
            {"model_hash": model_hash, "embedding_recipe_hash": recipe_hash, "recipe": recipe}
        )


def _embed_images(ctx: _Ctx, adapter) -> np.ndarray:
    mode = ctx.spec.image_input_mode()
    paths = [r.image_path for r in ctx.unique]
    if mode == "source" or ctx.media is None:
        return adapter.embed_paths(paths)
    from forge import image_backends
    from forge.image_corpus import _embed_compressed

    media_dir = image_media.media_dir_for(ctx.out_dir / f"{ctx.spec.name}.nest")
    frames_fn = image_backends.decoded_frames_fn(media_dir, ctx.media, ctx.frame_uris)
    vecs, _hashes = _embed_compressed(adapter, frames_fn, len(ctx.unique))
    perm = ctx.media.get("order_permutation")
    if perm:  # stream order -> item order (same inverse as image_corpus.build_corpus)
        vecs = vecs[np.argsort(np.asarray(perm))]
    return vecs
