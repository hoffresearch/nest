"""Greedy nearest-neighbour ordering of frames before the encode.

Measured in fase 0: ordering a corpus by visual similarity buys about +6.6
percent compression on unrelated images, because ordering cannot invent
redundancy that is not there. So it is a flag, never the default. Where it
can matter is scan-ordered material (wsi tiles), and that is where fase 6
measures it.

The permutation contract: `order[k]` is the original index of the k-th
frame in the encoded stream. The builder encodes in stream order and then
maps vectors, hashes, and uris back to item order, so a chunk always names
its own image no matter where the stream carried it.
"""

from __future__ import annotations

import numpy as np


def similarity_order(vectors: np.ndarray) -> list[int]:
    """Greedy NN walk over cosine similarity: order[k] = item at stream pos k.

    Starts at item 0 and repeatedly jumps to the most similar unvisited
    item. O(n^2) in time, one similarity row at a time, so no n-by-n matrix
    is ever materialized (a 10k corpus would need 400 MB for one).
    """
    n = len(vectors)
    if n < 3:
        return list(range(n))
    v = np.asarray(vectors, dtype=np.float32)
    norms = np.linalg.norm(v, axis=1, keepdims=True)
    v = v / np.where(norms == 0, 1.0, norms)
    visited = np.zeros(n, dtype=bool)
    order = [0]
    visited[0] = True
    current = 0
    for _ in range(n - 1):
        sims = v @ v[current]
        sims[visited] = -np.inf
        current = int(np.argmax(sims))
        visited[current] = True
        order.append(current)
    return order
