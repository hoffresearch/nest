"""Named embedding-model presets: models are data, not per-project code.

A preset declares what a model IS (ids, dims, validated MRL ladder, modalities),
what it NEEDS (imports with the exact pip fix line), and how it is USED by
default (the asymmetric query/document contract). The build spec may override
the usage fields; everything that changes an embedding output belongs to the
embedding_recipe_hash (RFC-0 N2), while `model_hash` identifies the model
alone.

MRL: `mrl.dims` is the ladder the model card validates. Slicing at any other
dim is mathematically possible and semantically unsupported, so it is refused
- a generic "slice anywhere" would be false advertising (RFC-3).

remote code: a preset with `trust_remote_code` loads ONLY when the caller
passes its name in `allow_remote_code` AND every code file matches the pinned
`remote_code_hashes` allowlist; the opt-in is the consent, the pin the identity.

hub snapshot: an open_clip preset with `hf_repo` and `revision` loads from
`snapshots/<revision>` of the hf cache, every file in `snapshot_files` present,
never through `refs/main`; a missing file is a typed SnapshotMissing naming it
and the fetch line (URNA_ALLOW_DOWNLOAD=1 fetches the pinned files once).
"""

from __future__ import annotations

import hashlib
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

# offline by default, the guarantee the forge package gave every registry
# import before the move: no hub round-trip unless the caller opts in. set
# here because every model load goes through the registry, and importing
# urna.embed no longer loads potiontab (which sets the same flags).
# tokenizers parallelism stays off, as potiontab sets it, whatever the opt-in.
if os.environ.get("URNA_ALLOW_DOWNLOAD") != "1":
    for _k in ("HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_DATASETS_OFFLINE"):
        os.environ.setdefault(_k, "1")
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

if TYPE_CHECKING:
    import numpy as np


class RegistryError(ValueError):
    """Typed registry failure: unknown preset, missing dep, refused load."""


class CapabilityError(RuntimeError):
    """The preset does not support the requested modality."""


class SnapshotMissing(FileNotFoundError):
    """A file of a preset's pinned hub snapshot is not on disk."""

    def __init__(self, message: str, repo: str):
        super().__init__(message)
        self.repo = repo


@dataclass(frozen=True)
class MrlSpec:
    supported: bool = False
    dims: tuple[int, ...] = ()
    method: str = "prefix_slice_l2"


@dataclass(frozen=True)
class InstallSpec:
    """A validated install: the hub revision, the exact files fetched at it
    (path, bytes), the `model_hash` those files fingerprint to, and the pip
    specs its backend needs. a preset carries one only after an install of
    exactly this was run end to end and its hash checked; `catalogue`
    exposes only those. the file list is part of the identity: the
    fingerprint hashes every relevant file present, so fetching one more
    weights file (a pytorch_model.bin beside model.safetensors) would change
    the model_hash."""

    revision: str
    files: tuple[tuple[str, int], ...]
    model_hash: str
    packages: tuple[str, ...]


@dataclass(frozen=True)
class ModelPreset:
    name: str
    kind: str  # "potion" | "open_clip" | "st_multimodal" | "st_text" | "fake"
    embedding_model: str  # manifest name; unique across presets (reverse lookup)
    model_id: str = ""
    pretrained: str | None = None
    default_dim: int = 0  # 0 = probe at load
    mrl: MrlSpec = field(default_factory=MrlSpec)
    modalities: frozenset[str] = frozenset({"text"})
    requires: tuple[tuple[str, str], ...] = ()  # (import_name, pip fix line)
    trust_remote_code: bool = False
    remote_code_hashes: tuple[tuple[str, str], ...] = ()  # (filename, sha256)
    local_dir: str | None = None
    executable: bool = True  # False: registered but refused without allow_heavy
    # asymmetric usage contract defaults (spec may override; all in recipe_hash)
    text_corpus_mode: str = "plain"  # "plain" | "document" | "query"
    text_query_mode: str = "plain"
    image_mode: str = "image_document"
    image_prompt: str = "Represent this image."
    normalize: bool = True
    preprocess_version: str = "processor-native-v1"
    image_max_side: int = 0  # 0 = model-native; recipe-hashed preprocessing
    # how an image document is handed to ST encode(): "dict" = {"image", "text": prompt}
    # (wemm's contract); "bare" = the PIL image alone (jina: the dict form collapses
    # its image embeddings onto the shared prompt text - measured, sims 0.99 across
    # different images vs 0.45 bare). recipe-hashed.
    image_doc_format: str = "dict"
    encode_kwargs: tuple[tuple[str, object], ...] = ()  # ST encode() extras (recipe-hashed)
    install: InstallSpec | None = None  # validated install; None = not offered by setup
    # the hub snapshot an open_clip preset loads from: repo, pinned commit, the
    # checkpoint file and every file the load and the tokenizer read.
    hf_repo: str = ""
    revision: str = ""
    weights_file: str = ""
    snapshot_files: tuple[str, ...] = ()


_ST_REQUIRES = (
    ("torch", "pip install torch"),
    ("sentence_transformers", 'pip install "sentence-transformers>=5.7"'),
    ("transformers", 'pip install "transformers==5.2.0"'),
)
_WEMM_REQUIRES = _ST_REQUIRES + (("qwen_vl_utils", 'pip install "qwen-vl-utils==0.0.14"'),)
_OPEN_CLIP_REQUIRES = (
    ("torch", "pip install torch"),
    ("open_clip", "pip install open_clip_torch"),
    ("PIL", "pip install pillow"),
)


# pinned sha256 of the wemm-2b remote-code files as reviewed 2026-08-31
# (N11): a changed file is REFUSED, not warned about. 4b/9b have no local
# snapshot yet, so their pin list stays empty until one is reviewed.
_WEMM_2B_CODE_PINS = (
    ("modeling_st_wemm.py", "521d02c1c60ae727cc9dc6500cdb0b28c53b259e0ce3d37197920a33ba4dd333"),
    (
        "modeling_wemm_embedding.py",
        "ac255e1fad459cc3e68891d6c3327f4486922aed02fb3c5c13fb53277ba8e94f",
    ),
    ("chat_template.jinja", "273d8e0e683b885071fb17e08d71e5f2a5ddfb5309756181681de4f5a1822d80"),
    (
        "embedding_chat_template.jinja",
        "7c3df2aab83ab9096428ec27b6b99ad87c4790418b830d119957634f28c677ba",
    ),
    ("processor_config.json", "d601e2fe0de1bc11852de3aa843f01a1677ca84f4dc743916c9ce4b8d30fb384"),
)


def _wemm(name: str, size: str, dim: int, executable: bool, local_dir: str | None) -> ModelPreset:
    return ModelPreset(
        name=name,
        kind="st_multimodal",
        embedding_model=f"tencent/WeMM-Embedding-{size}",
        model_id=f"tencent/WeMM-Embedding-{size}",
        default_dim=dim,
        mrl=MrlSpec(True, (128, 256, 512, 1024, dim)),
        modalities=frozenset({"text", "image", "video"}),
        requires=_WEMM_REQUIRES,
        trust_remote_code=True,
        remote_code_hashes=_WEMM_2B_CODE_PINS if name == "wemm-2b" else (),
        local_dir=local_dir,
        executable=executable,
        text_corpus_mode="document",
        text_query_mode="query",
        image_max_side=768,
    )


PRESETS: dict[str, ModelPreset] = {
    p.name: p
    for p in (
        ModelPreset(
            name="potion",
            kind="potion",
            embedding_model="minishlab/potion-base-8M/v1",
            model_id="minishlab/potion-base-8M",
            default_dim=256,
            requires=(("numpy", "pip install numpy"), ("tokenizers", "pip install tokenizers")),
        ),
        ModelPreset(
            name="clip-vit-b32",
            kind="open_clip",
            embedding_model="open_clip/ViT-B-32/openai",
            model_id="ViT-B-32",
            pretrained="openai",
            default_dim=512,
            modalities=frozenset({"text", "image"}),
            requires=_OPEN_CLIP_REQUIRES,
        ),
        ModelPreset(
            name="siglip2",
            kind="open_clip",
            embedding_model="open_clip/ViT-B-16-SigLIP2/webli",
            model_id="ViT-B-16-SigLIP2",
            pretrained="webli",
            default_dim=768,
            modalities=frozenset({"text", "image"}),
            # its text tower tokenizes through transformers' AutoTokenizer
            requires=_OPEN_CLIP_REQUIRES + (("transformers", 'pip install "transformers==5.2.0"'),),
            # the revision whose weights fingerprint to the model_hash of the
            # mtg stills-5models build (sha256:a9946db1...), checked 2026-10-02
            hf_repo="timm/ViT-B-16-SigLIP2",
            revision="eee10eff6dd8cabae2d7f379d4e8cfcd352030aa",
            weights_file="open_clip_model.safetensors",
            snapshot_files=(
                "open_clip_model.safetensors",
                "open_clip_config.json",
                "tokenizer.json",
                "tokenizer_config.json",
                "special_tokens_map.json",
            ),
        ),
        ModelPreset(
            name="jina-v5-omni-nano",
            kind="st_multimodal",
            embedding_model="jinaai/jina-embeddings-v5-omni-nano",
            model_id="jinaai/jina-embeddings-v5-omni-nano",
            mrl=MrlSpec(True, (32, 64, 128, 256, 512, 768)),
            modalities=frozenset({"text", "image", "video"}),
            requires=_ST_REQUIRES,
            trust_remote_code=True,
            text_corpus_mode="document",
            text_query_mode="query",
            image_doc_format="bare",
            encode_kwargs=(("task", "retrieval"),),
        ),
        ModelPreset(
            name="jina-v5-omni-small",
            kind="st_multimodal",
            embedding_model="jinaai/jina-embeddings-v5-omni-small",
            model_id="jinaai/jina-embeddings-v5-omni-small",
            mrl=MrlSpec(True, (32, 64, 128, 256, 512, 768, 1024)),
            modalities=frozenset({"text", "image", "video"}),
            requires=_ST_REQUIRES,
            trust_remote_code=True,
            text_corpus_mode="document",
            text_query_mode="query",
            image_doc_format="bare",
            encode_kwargs=(("task", "retrieval"),),
        ),
        # the multilingual MiniLM the pt-br corpora were built with, before
        # the registry existed. kind st_text loads it through rust/bridge/python/urna/
        # searchtxt.py's own functions, so a corpus built then keeps its
        # model_hash. no transformers pin: the st pin above is wemm's.
        ModelPreset(
            name="minilm-multilingual",
            kind="st_text",
            embedding_model="sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
            model_id="sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
            default_dim=384,
            requires=(("sentence_transformers", 'pip install "sentence-transformers"'),),
            # the revision and files the pt-br corpora were built from (the
            # files sentence-transformers itself fetches; the repo also holds
            # onnx, openvino and a pytorch_model.bin that are never fetched).
            install=InstallSpec(
                revision="e8f8c211226b894fcb81acc59f3b34ba3efd5f42",
                files=(
                    ("1_Pooling/config.json", 190),
                    ("README.md", 3888),
                    ("config.json", 645),
                    ("config_sentence_transformers.json", 122),
                    ("model.safetensors", 470641600),
                    ("modules.json", 229),
                    ("sentence_bert_config.json", 53),
                    ("special_tokens_map.json", 239),
                    ("tokenizer.json", 9081518),
                    ("tokenizer_config.json", 526),
                ),
                model_hash="sha256:97a500de1ad2dc9ffb41fbe3dec27d4fd26ddaa13143aa50f905d3d1f6eff0af",
                packages=("sentence-transformers>=3",),
            ),
        ),
        _wemm(
            "wemm-2b",
            "2B",
            2048,
            executable=True,
            local_dir="~/models/modelsdownload/WeMM-Embedding-2B",
        ),
        _wemm("wemm-4b", "4B", 2560, executable=False, local_dir=None),
        _wemm("wemm-9b", "9B", 4096, executable=False, local_dir=None),
        ModelPreset(
            name="fake-test",
            kind="fake",
            embedding_model="urna-forge-fake-test/v1",
            default_dim=8,
            mrl=MrlSpec(True, (8, 4)),
            modalities=frozenset({"text", "image"}),
        ),
    )
}


def get_preset(name: str) -> ModelPreset:
    if name == "fake-test" and os.environ.get("URNA_ENABLE_FAKE_PRESET") != "1":
        raise RegistryError("preset 'fake-test' requires URNA_ENABLE_FAKE_PRESET=1 (test-only)")
    preset = PRESETS.get(name)
    if preset is None:
        valid = ", ".join(sorted(PRESETS))
        raise RegistryError(f"unknown model preset '{name}'. valid presets: {valid}")
    return preset


def preset_for_embedding_model(embedding_model: str) -> ModelPreset | None:
    """Reverse lookup for the query embedder: manifest name -> preset."""
    for preset in PRESETS.values():
        if preset.embedding_model == embedding_model:
            return preset
    return None


def check_deps(preset: ModelPreset) -> None:
    """Raise RegistryError naming the exact pip line for the first missing dep."""
    import importlib.util

    for module, fix in preset.requires:
        if importlib.util.find_spec(module) is None:
            raise RegistryError(
                f"preset '{preset.name}' needs the '{module}' package. install with: {fix}"
            )


def resolve_model_dir(preset: ModelPreset, model_path: str | os.PathLike | None = None):
    """explicit > URNA_MODEL_DIR_<NAME> env > preset.local_dir > hf cache > None."""
    if model_path:
        return Path(model_path)
    env_key = "URNA_MODEL_DIR_" + preset.name.upper().replace("-", "_")
    if os.environ.get(env_key):
        return Path(os.environ[env_key])
    if preset.local_dir:
        local = Path(preset.local_dir).expanduser()
        if local.is_dir():
            return local
    if preset.kind == "st_multimodal":
        from urna.model.modelhash import hf_cache_snapshot

        try:
            return hf_cache_snapshot(preset.model_id)
        except (FileNotFoundError, ValueError, RuntimeError):
            return None
    return None


def pinned_snapshot(preset: ModelPreset, model_path: str | os.PathLike | None = None) -> Path:
    """The directory holding the preset's pinned hub snapshot, every file present.

    explicit model_path > URNA_MODEL_DIR_<NAME> > `snapshots/<revision>` in the
    hf cache. refs/main is never read: it names whatever was fetched last, not
    the revision the preset verified. A missing file raises SnapshotMissing,
    unless URNA_ALLOW_DOWNLOAD=1, which fetches the pinned files first.
    """
    env_key = "URNA_MODEL_DIR_" + preset.name.upper().replace("-", "_")
    explicit = model_path or os.environ.get(env_key)
    if explicit:
        snap = Path(explicit)
    else:
        hf_home = Path(os.environ.get("HF_HOME", Path.home() / ".cache" / "huggingface"))
        cache = hf_home / "hub" / f"models--{preset.hf_repo.replace('/', '--')}"
        snap = cache / "snapshots" / preset.revision
    missing = [f for f in preset.snapshot_files if not (snap / f).is_file()]
    if missing and not explicit and os.environ.get("URNA_ALLOW_DOWNLOAD") == "1":
        snap = _fetch_snapshot(preset)
        missing = [f for f in preset.snapshot_files if not (snap / f).is_file()]
    if missing:
        files = " ".join(preset.snapshot_files)
        fetch = f"hf download {preset.hf_repo} {files} --revision {preset.revision}"
        where = str(snap) if explicit else f"the hf cache ({preset.hf_repo}@{preset.revision[:12]})"
        raise SnapshotMissing(
            f"preset '{preset.name}': {', '.join(missing)} missing from {where}. fetch the pinned "
            f"files with: {fetch}, or run once with URNA_ALLOW_DOWNLOAD=1",
            preset.hf_repo,
        )
    return snap


def _fetch_snapshot(preset: ModelPreset) -> Path:
    """Download the preset's pinned files: the explicit URNA_ALLOW_DOWNLOAD=1 opt-in."""
    # importing potiontab defaults the hub offline; the opt-in wins,
    # as in stbackend. huggingface_hub reads the flag when it is first imported.
    for k in ("HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_DATASETS_OFFLINE"):
        os.environ.pop(k, None)
    from huggingface_hub import hf_hub_download

    path = None
    for f in preset.snapshot_files:
        path = hf_hub_download(preset.hf_repo, f, revision=preset.revision)
    return Path(path).parent


def verify_remote_code(preset: ModelPreset, model_dir: Path) -> None:
    """Pin gate (N11): every allowlisted code file must match its sha256."""
    for filename, expected in preset.remote_code_hashes:
        path = model_dir / filename
        if not path.is_file():
            raise RegistryError(f"preset '{preset.name}': pinned code file missing: {filename}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise RegistryError(
                f"preset '{preset.name}': {filename} sha256 {actual} does not match the "
                f"pinned allowlist ({expected}). refusing to execute unreviewed remote code."
            )


def slice_renorm(vecs: np.ndarray, dim: int) -> np.ndarray:
    """First-`dim` prefix slice + re-L2 (same semantics as build_fn's mrl_dim).
    numpy loads here, not at import: the catalog generator reads the
    registry under a bare python3 (the release's stage step)."""
    import numpy as np

    v = np.asarray(vecs, dtype=np.float32)
    if not 0 < dim <= v.shape[1]:
        raise ValueError(f"slice dim {dim} out of range for source dim {v.shape[1]}")
    sliced = np.ascontiguousarray(v[:, :dim])
    norms = np.linalg.norm(sliced, axis=1, keepdims=True)
    return (sliced / np.where(norms == 0, 1.0, norms)).astype(np.float32)


def create_embedder(
    name: str,
    *,
    model_path: str | os.PathLike | None = None,
    device: str | None = None,
    batch_size: int | None = None,
    allow_remote_code: frozenset[str] | set[str] = frozenset(),
    allow_heavy: bool = False,
    usage: dict | None = None,
):
    """Return a unified adapter (embed_texts/embed_paths/embed_arrays, dim,
    model_hash, fingerprint()) for the preset, enforcing the RFC-0 gates."""
    preset = get_preset(name)
    if not preset.executable and not allow_heavy:
        raise RegistryError(
            f"preset '{name}' is registered but flagged too heavy for this machine; "
            "pass --allow-heavy to load it anyway"
        )
    if preset.trust_remote_code and name not in allow_remote_code:
        raise RegistryError(
            f"preset '{name}' executes model-repo code (trust_remote_code). opt in "
            f'explicitly with allow_remote_code = ["{name}"] in the spec'
        )
    check_deps(preset)
    from urna.model import embedders as _adapters

    if preset.kind == "fake":
        return _adapters._FakeAdapter(preset)
    if preset.kind == "potion":
        from urna.embed import potiontab

        return _adapters._PotionAdapter(preset, potiontab.PotionEmbedder())
    if preset.kind == "open_clip":
        from urna.embed import visionemb

        snapshot = pinned_snapshot(preset, model_path) if preset.revision else None
        inner = visionemb.ImageEmbedder(
            model_id=preset.model_id,
            pretrained=preset.pretrained,
            device=device,
            batch_size=batch_size or 32,
            snapshot=snapshot,
            weights=preset.weights_file or None,
        )
        return _adapters._OpenClipAdapter(preset, inner)
    if preset.kind == "st_text":
        return _adapters._STTextAdapter(preset, model_path=model_path)
    if preset.kind == "st_multimodal":
        model_dir = resolve_model_dir(preset, model_path)
        if model_dir is not None and preset.remote_code_hashes:
            verify_remote_code(preset, model_dir)
        return _adapters._SubprocessSTAdapter(
            preset,
            model_dir=model_dir,
            model_path=model_path,
            device=device,
            batch_size=batch_size or 8,
            usage=usage or {},
        )
    raise RegistryError(f"preset '{name}': unknown kind '{preset.kind}'")
