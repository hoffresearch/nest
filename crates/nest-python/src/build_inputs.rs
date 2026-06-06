//! Input parsing and build-time vector conditioning for `build()`.
//!
//! Kept out of `build_fn.rs` so the entry point stays under the 300-line
//! crate guard. Two helpers: `parse_chunks` (PyList of dicts ->
//! `Vec<ChunkInput>`) and `truncate_renormalize` (matryoshka prefix slice +
//! L2-renorm applied before quantization/HNSW).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// lTruncate each row to its first `mrl_dim` components and re-L2-normalize
/// the prefix in place (matryoshka truncate-then-renormalize). `full_dim` is
/// the source dim; rows shorter than `full_dim` are left untouched (the
/// builder's per-chunk validation rejects a real dim mismatch later). A zero
/// or non-finite prefix norm leaves the (already truncated) row unscaled, so
/// the codec/validation path still sees a well-formed-length vector and
/// reports the problem with a typed error rather than producing NaNs.
pub(crate) fn truncate_renormalize(
    chunks: &mut [nest_format::ChunkInput],
    full_dim: usize,
    mrl_dim: usize,
) {
    for c in chunks.iter_mut() {
        if c.embedding.len() != full_dim {
            continue;
        }
        c.embedding.truncate(mrl_dim);
        let norm: f32 = c.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 && norm.is_finite() {
            for x in &mut c.embedding {
                *x /= norm;
            }
        }
    }
}

/// lBuild the chunk-to-chunk graph_adjacency (0x0C) csr payload for `n`
/// chunks: NEXT_CHUNK edges (sequential ordinals, both directions so the
/// bounded bfs can reconstruct neighbor context on either side) plus up to
/// `top_m` SEMANTIC edges per node taken from the already-built hnsw level-0
/// adjacency. canonically sorted by `encode_graph_adjacency` (ascending src,
/// edge_type, dst) so two builds are byte-identical. returns `None` when
/// there is nothing to emit (n < 2). deterministic and pure.
pub(crate) fn build_graph_payload(
    hnsw: Option<&nest_runtime::ann::HnswIndex>,
    n: usize,
    top_m: usize,
) -> PyResult<Option<Vec<u8>>> {
    use nest_format::{EDGE_TYPE_NEXT_CHUNK, EDGE_TYPE_SEMANTIC, Edge, encode_graph_adjacency};
    if n < 2 {
        return Ok(None);
    }
    let mut edges: Vec<Edge> = Vec::new();
    // NEXT_CHUNK: i <-> i+1 (sequential reading order, both directions).
    for i in 0..n - 1 {
        edges.push(Edge {
            src: i as u32,
            dst: (i + 1) as u32,
            edge_type: EDGE_TYPE_NEXT_CHUNK,
        });
        edges.push(Edge {
            src: (i + 1) as u32,
            dst: i as u32,
            edge_type: EDGE_TYPE_NEXT_CHUNK,
        });
    }
    // SEMANTIC: top-m from the hnsw level-0 graph (already built, no o(n^2)
    // knn). skip self-loops; the encoder dedups by canonical sort, so a node
    // appearing in both a NEXT_CHUNK and a SEMANTIC edge keeps both typed
    // edges (different edge_type = different canonical key).
    if let Some(idx) = hnsw {
        for i in 0..n {
            let mut count = 0usize;
            for &nbr in idx.level0_neighbors(i) {
                if count >= top_m {
                    break;
                }
                if nbr as usize == i {
                    continue;
                }
                edges.push(Edge {
                    src: i as u32,
                    dst: nbr,
                    edge_type: EDGE_TYPE_SEMANTIC,
                });
                count += 1;
            }
        }
    }
    let payload = encode_graph_adjacency(n, &edges)
        .map_err(|e| PyValueError::new_err(format!("graph_adjacency encode: {}", e)))?;
    Ok(Some(payload))
}

pub(crate) fn parse_chunks(chunks: &Bound<PyList>) -> PyResult<Vec<nest_format::ChunkInput>> {
    use nest_format::ChunkInput;
    let mut out: Vec<ChunkInput> = Vec::with_capacity(chunks.len());
    for (i, item) in chunks.iter().enumerate() {
        let d: Bound<PyDict> = item
            .cast::<PyDict>()
            .map_err(|_| PyValueError::new_err(format!("chunks[{}] is not a dict", i)))?
            .clone();
        let d = &d;
        let canonical_text: String = d
            .get_item("canonical_text")?
            .ok_or_else(|| PyValueError::new_err(format!("chunks[{}] missing canonical_text", i)))?
            .extract()?;
        let source_uri: String = d
            .get_item("source_uri")?
            .ok_or_else(|| PyValueError::new_err(format!("chunks[{}] missing source_uri", i)))?
            .extract()?;
        let byte_start: u64 = d
            .get_item("byte_start")?
            .ok_or_else(|| PyValueError::new_err(format!("chunks[{}] missing byte_start", i)))?
            .extract()?;
        let byte_end: u64 = d
            .get_item("byte_end")?
            .ok_or_else(|| PyValueError::new_err(format!("chunks[{}] missing byte_end", i)))?
            .extract()?;
        let embedding: Vec<f32> = d
            .get_item("embedding")?
            .ok_or_else(|| PyValueError::new_err(format!("chunks[{}] missing embedding", i)))?
            .extract()?;
        out.push(ChunkInput {
            canonical_text,
            source_uri,
            byte_start,
            byte_end,
            embedding,
        });
    }
    Ok(out)
}
