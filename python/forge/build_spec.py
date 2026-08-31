"""Declarative build spec: parse + validate the TOML/JSON contract (RFC-1).

Every user-facing choice lives here as a typed field with its default; every
violation raises SpecError naming the offending key. Unknown keys are errors,
not silence — a typo'd knob that silently does nothing is a lie in a config.
"""

from __future__ import annotations

import json
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


@dataclass
class MediaSpec:
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
    return _parse(data, str(path))


def _parse(data: dict, spec_path: str) -> CorpusSpec:
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
        quality = _section(QualitySpec, dict(m.pop("quality", {})), "media.quality")
        cluster = _section(ClusterSpec, dict(m.pop("cluster", {})), "media.cluster")
        jxl = _section(JxlTranscodeSpec, dict(m.pop("jxl_transcode", {})), "media.jxl_transcode")
        media = _section(MediaSpec, m, "media")
        media.quality, media.cluster, media.jxl_transcode = quality, cluster, jxl

    models = [_section(ModelSpec, dict(m), "models") for m in data.get("models", [])]
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
    "load_spec",
    "default_model",
    "emitted_spaces",
    "validate",
]
