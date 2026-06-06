"""forge: the python surface of the ingestion layer.

this phase ships the flagship blockers: the offline DEFAULT embedder and a
license-clean demo corpus. the default is now the REAL semantic static table
(model2vec/potion-base-8M, offline, no torch); the #04 lexical bag-of-words
stays available as the zero-dependency floor (lexical_embedder). the .fci
adapter and the forge cli are forge-0b (a later card) and reuse the one
authoritative chunker (builder.chunk_text); they are not part of this package
yet.
"""

from forge.embed_default import StaticEmbedder
from forge.embed_default import default_embedder as lexical_embedder
from forge.embed_potion import PotionEmbedder, default_embedder, potion_embedder

__all__ = [
    "StaticEmbedder",
    "PotionEmbedder",
    "default_embedder",
    "potion_embedder",
    "lexical_embedder",
]
