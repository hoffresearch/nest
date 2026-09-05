"""Declarative build spec: parse + validate the TOML/JSON contract (RFC-1).

Every user-facing choice lives here as a typed field with its default; every
violation raises SpecError naming the offending key. Unknown keys are errors,
not silence — a typo'd knob that silently does nothing is a lie in a config.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field, fields
from pathlib import Path


class SpecError(ValueError):
    """Spec violation; message names the offending key."""


def _section(cls, data: dict, where: str):
    """Build a dataclass from a dict, refusing unknown keys."""
    allowed = {f.name for f in fields(cls)}
    for key in data:
        if key not in allowed:
            raise SpecError(f"{where}: unknown key '{key}' (valid: {', '.join(sorted(allowed))})")
    return cls(**data)


@dataclass
class SourceJoin:
    query: str
    on: str


@dataclass
class SourceText:
    template: str = ""


@dataclass
class SourceImage:
    path_template: str = ""
    label_template: str = ""


@dataclass
class SourceSpec:
    kind: str = ""
    db: str = ""
    query: str = ""
    order_by: list[str] = field(default_factory=list)
    derive: dict[str, str] = field(default_factory=dict)
    joins: list[SourceJoin] = field(default_factory=list)
    text: SourceText = field(default_factory=SourceText)
    image: SourceImage = field(default_factory=SourceImage)
    input_dir: str = ""  # image_dir | pdf_dir
    labels: str = ""  # image_dir | pdf_dir label file
    path: str = ""  # csv | jsonl


@dataclass
class ImageInputSpec:
    mode: str = ""  # "" = decoded_media when [media] present, else source


@dataclass
class QualitySpec:
    strategy: str = "stratified"
    buckets: list[str] = field(
        default_factory=lambda: ["resolution", "entropy", "has_text", "alpha", "source_format"]
    )
    sample_per_bucket: int = 12
    visual_floor_p10: float = 85.0
    visual_floor_min: float = 72.0
    drift_floor_p10: float = 0.98
    gate_model: str = ""
    crf_ladder: list[int] = field(default_factory=lambda: [30, 35, 40, 45])


@dataclass
class ClusterSpec:
    space: str = ""
    threshold: float = 0.92


@dataclass
class JxlTranscodeSpec:
    on_unsupported_jpeg: str = "copy-source"  # error | copy-source | lossless-jxl
    verify_roundtrip: bool = True
    keep_metadata: bool = False


# dataset profiles: measured recipes (bench 2026-08-31) resolved into knob
# defaults BEFORE explicit keys are applied — an explicit key always wins,
# so no use case is closed off by choosing one.
#   near-dup: corpora with visual near-duplicates (card reprints, frames,
#             scans): cluster ordering + per-segment gop lets inter pay
#             (-29% on same-artwork reprints) without losing O(1) access
#             on unique segments.
#   stills:   unique images: all-intra + still tune (best size at O(1) seek).
#   archive:  byte-reversible JPEG repack (jxl-transcode, 1.12x, sha256-
#             verified roundtrip): for corpora where loss is not acceptable.
MEDIA_PROFILES: dict[str, dict] = {
    "near-dup": {"order": "cluster", "gop": "auto", "tune": "still"},
    "stills": {"gop": "intra", "tune": "still"},
    "archive": {"backend": "jxl-transcode"},
}


@dataclass
class MediaSpec:
    profile: str = ""  # "" | near-dup | stills | archive (see MEDIA_PROFILES)
    backend: str = "av1"  # av1 | avif | jxl | jxl-transcode | control
    width: int = 1024
    crf: int | str = 35  # int | "auto"
    tune: str = "default"  # default | still
    speed: int = 8
    fps: int = 1
    pix_fmt: str = "yuv420p"
    shard_size: int = 2048
    gop: str = "auto"  # auto | intra | inter
    order: str = "none"  # none | similarity | cluster
    dedup: bool = True
    quality: QualitySpec = field(default_factory=QualitySpec)
    cluster: ClusterSpec = field(default_factory=ClusterSpec)
    jxl_transcode: JxlTranscodeSpec = field(default_factory=JxlTranscodeSpec)


@dataclass
class ModelSpec:
    preset: str = ""
    text: str = "none"  # default | space | none
    image: str = "none"  # space | none
    dims: list[int] = field(default_factory=list)
    model_path: str = ""
    device: str = ""
    batch_size: int = 32
    dtype: str = ""
    space_dtype: str = "int8"
    text_corpus_mode: str = ""  # "" = preset default
    text_query_mode: str = ""
    image_mode: str = ""
    image_prompt: str = ""
    normalize: bool = True
    preprocess_version: str = ""
    image_max_side: int = 0  # 0 = preset default
    encode_kwargs: dict = field(default_factory=dict)


@dataclass
class EngineBuildSpec:
    preset: str = "hybrid"
    dtype: str = ""
    with_graph: bool = True
    graph_top_m: int = 8
    graph_space: str = "default"
    mrl_dim: int = 0


@dataclass
class OutputSpec:
    mode: str = "single"  # single | per-model | both
    dir: str = "out"
    provenance: str = "standard"  # minimal | standard | full
    allow_remote_code: list[str] = field(default_factory=list)
    # inline the media bytes into the .nest (0x17): one self-contained file,
    # no sidecar needed at read time. the media dir remains as build cache.
    embed_media: bool = False


@dataclass
class CorpusSpec:
    name: str = ""
    title: str = ""
    version: str = "0.1.0"
    chunker_version: str = ""
    reproducible: bool = True
    source: SourceSpec = field(default_factory=SourceSpec)
    image_input: ImageInputSpec = field(default_factory=ImageInputSpec)
    media: MediaSpec | None = None
    models: list[ModelSpec] = field(default_factory=list)
    build: EngineBuildSpec = field(default_factory=EngineBuildSpec)
    output: OutputSpec = field(default_factory=OutputSpec)
    spec_path: str = ""

    def image_input_mode(self) -> str:
        if self.image_input.mode:
            return self.image_input.mode
        return "decoded_media" if self.media is not None else "source"


MAX_SPACES = 15  # SPACE_BAND_LEN - 1


def load_spec(path: str | Path) -> CorpusSpec:
    path = Path(path)
    raw = path.read_bytes()
    if path.suffix == ".toml":
        import tomllib

        data = tomllib.loads(raw.decode())
    elif path.suffix == ".json":
        data = json.loads(raw)
    elif path.suffix in (".yaml", ".yml"):
        try:
            import yaml
        except ImportError as e:
            raise SpecError("yaml specs need pyyaml: pip install pyyaml (or use .toml)") from e
        data = yaml.safe_load(raw)
    else:
        raise SpecError(f"unsupported spec extension '{path.suffix}' (use .toml or .json)")
    return _parse(_expand_home(data), str(path))


def _expand_home(node):
    # specs stay machine-portable: "~/..." instead of a hardcoded home.
    if isinstance(node, str):
        return os.path.expanduser(node) if node.startswith("~/") else node
    if isinstance(node, dict):
        return {k: _expand_home(v) for k, v in node.items()}
    if isinstance(node, list):
        return [_expand_home(v) for v in node]
    return node


_KNOWN_TABLES = frozenset({"corpus", "source", "media", "models", "embedding", "build", "output"})


def _parse(data: dict, spec_path: str) -> CorpusSpec:
    unknown = sorted(set(data) - _KNOWN_TABLES)
    if unknown:
        raise SpecError(
            f"unknown top-level table(s) {unknown}: valid tables are {sorted(_KNOWN_TABLES)}"
        )
    raw_models = data.get("models", [])
    if not isinstance(raw_models, list) or not all(isinstance(m, dict) for m in raw_models):
        raise SpecError("models must be an array of tables — write [[models]], not [models]")
    corpus = dict(data.get("corpus", {}))
    src = dict(data.get("source", {}))
    joins = [_section(SourceJoin, dict(j), "source.joins") for j in src.pop("joins", [])]
    text = _section(SourceText, dict(src.pop("text", {})), "source.text")
    image = _section(SourceImage, dict(src.pop("image", {})), "source.image")
    order_by = src.pop("order_by", [])
    if isinstance(order_by, str):
        order_by = [order_by]
    source = _section(SourceSpec, {**src, "order_by": order_by}, "source")
    source.joins, source.text, source.image = joins, text, image

    media = None
    if "media" in data:
        m = dict(data["media"])
        profile = m.pop("profile", "")
        if profile:
            if profile not in MEDIA_PROFILES:
                raise SpecError(
                    f"media.profile: unknown '{profile}' (valid: {sorted(MEDIA_PROFILES)})"
                )
            m = {**MEDIA_PROFILES[profile], **m, "profile": profile}
        quality = _section(QualitySpec, dict(m.pop("quality", {})), "media.quality")
        cluster = _section(ClusterSpec, dict(m.pop("cluster", {})), "media.cluster")
        jxl = _section(JxlTranscodeSpec, dict(m.pop("jxl_transcode", {})), "media.jxl_transcode")
        media = _section(MediaSpec, m, "media")
        media.quality, media.cluster, media.jxl_transcode = quality, cluster, jxl

    models = [_section(ModelSpec, dict(m), "models") for m in raw_models]
    image_input = _section(
        ImageInputSpec,
        dict(data.get("embedding", {}).get("image_input", {})),
        "embedding.image_input",
    )
    build = _section(EngineBuildSpec, dict(data.get("build", {})), "build")
    output = _section(OutputSpec, dict(data.get("output", {})), "output")
    spec = _section(CorpusSpec, corpus, "corpus")
    spec.source, spec.image_input, spec.media = source, image_input, media
    spec.models, spec.build, spec.output, spec.spec_path = models, build, output, spec_path
    return spec


# re-exported rules: callers import the whole contract from build_spec.
from forge.spec_rules import default_model, emitted_spaces, validate  # noqa: E402

__all__ = [
    "CorpusSpec",
    "SourceSpec",
    "MediaSpec",
    "ModelSpec",
    "SpecError",
    "MAX_SPACES",
    "MEDIA_PROFILES",
    "load_spec",
    "default_model",
    "emitted_spaces",
    "validate",
]
