"""forge: the python surface of the ingestion layer.

this phase ships the flagship blockers (#04): the DEFAULT offline static
embedder and a license-clean demo corpus. the .fci adapter and the forge cli
are forge-0b (a later card) and reuse the one authoritative chunker
(builder.chunk_text); they are not part of this package yet.
"""

from forge.embed_default import StaticEmbedder, default_embedder

__all__ = ["StaticEmbedder", "default_embedder"]
