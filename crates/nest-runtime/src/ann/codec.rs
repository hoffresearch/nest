//! On-disk encoding/decoding for the HNSW section (`0x07`, `encoding=raw`).
//!
//! Payload version 2 bitpacks the graph with the shared `intpack` codec:
//! after the fixed header, three length-prefixed `intpack` columns hold the
//! per-node level, the per-layer neighbour counts, and every neighbour id
//! (in build order). intpack is order-preserving, so the decoded graph is
//! identical to the built one and recall is unchanged; the win is the
//! neighbour ids dropping from a flat u32 to ~`ceil(log2 n)` bits each.
//!
//! Version 1 (flat u32 per id) is still accepted on read so existing files
//! keep opening. The section is optional and excluded from content_hash.
//!
//! Section checksum (8 bytes of SHA-256 over the physical bytes) is
//! computed by the writer at file-build time, not here.

use nest_format::encoding::{pack_u64s, unpack_u64s};
use nest_format::error::NestError;

use super::{HNSW_PAYLOAD_VERSION, HnswIndex, Node};
use crate::error::RuntimeError;
use crate::materialize::PackedVectors;

impl HnswIndex {
    /// lEncode the index to bytes for embedding in section `0x07` (v2).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(7 * 4 + self.n * 4);
        out.extend_from_slice(&HNSW_PAYLOAD_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.m as u32).to_le_bytes());
        out.extend_from_slice(&(self.m_max0 as u32).to_le_bytes());
        out.extend_from_slice(&(self.ef_construction as u32).to_le_bytes());
        out.extend_from_slice(&self.entry_point.to_le_bytes());
        out.extend_from_slice(&self.max_level.to_le_bytes());
        out.extend_from_slice(&(self.n as u32).to_le_bytes());

        let mut levels: Vec<u64> = Vec::with_capacity(self.nodes.len());
        let mut counts: Vec<u64> = Vec::new();
        let mut ids: Vec<u64> = Vec::new();
        for node in &self.nodes {
            levels.push(node.level as u64);
            for layer in 0..=node.level {
                let nbrs = &node.neighbors[layer as usize];
                counts.push(nbrs.len() as u64);
                ids.extend(nbrs.iter().map(|&id| id as u64));
            }
        }
        for col in [&levels, &counts, &ids] {
            let blob = pack_u64s(col);
            out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            out.extend_from_slice(&blob);
        }
        out
    }

    /// lParse an HNSW payload (v1 or v2). The vectors are reconstructed
    /// from the embeddings section by the caller; this constructor is for
    /// the on-disk graph only. Call `attach_vectors` before search.
    pub fn from_bytes(bytes: &[u8], n: usize, dim: usize) -> Result<Self, RuntimeError> {
        let mut cur = ByteCursor::new(bytes);
        let version = cur.u32()?;
        let m = cur.u32()? as usize;
        let m_max0 = cur.u32()? as usize;
        let ef_construction = cur.u32()? as usize;
        let entry_point = cur.u32()?;
        let max_level = cur.u32()?;
        let n_nodes = cur.u32()? as usize;
        if n_nodes != n {
            return Err(malformed(format!(
                "node count {} != n_embeddings {}",
                n_nodes, n
            )));
        }
        let nodes = match version {
            1 => decode_nodes_v1(&mut cur, n_nodes)?,
            2 => decode_nodes_v2(&mut cur, n_nodes)?,
            other => {
                return Err(RuntimeError::Format(NestError::UnsupportedSectionVersion {
                    section_id: nest_format::layout::SECTION_HNSW_INDEX,
                    version: other,
                }));
            }
        };
        Ok(Self {
            m,
            m_max0,
            ef_construction,
            entry_point,
            max_level,
            nodes,
            store: PackedVectors::empty(),
            dim,
            n,
            ef_search: ef_construction,
        })
    }
}

fn malformed(reason: impl Into<String>) -> RuntimeError {
    RuntimeError::Format(NestError::MalformedSectionPayload {
        section_id: nest_format::layout::SECTION_HNSW_INDEX,
        reason: reason.into(),
    })
}

// hostile-input caps. a real hnsw never approaches these (levels are
// geometric ~log_M(n); degree <= m_max0 ~ 2*M), so they never reject a
// valid file. they bound the decoded-but-untrusted level/degree VALUES so a
// crafted payload cannot overflow the count sums or force a giant
// allocation before the column-length cross-checks run.
const MAX_LEVEL: u64 = 64;
const MAX_DEGREE: u64 = 4096;

fn check_cap(values: &[u64], cap: u64, what: &str) -> Result<(), RuntimeError> {
    if values.iter().any(|&v| v > cap) {
        return Err(malformed(format!("hnsw: {} exceeds cap {}", what, cap)));
    }
    Ok(())
}

/// v1: flat `u32` per id, inline per node.
fn decode_nodes_v1(cur: &mut ByteCursor, n_nodes: usize) -> Result<Vec<Node>, RuntimeError> {
    let mut nodes = Vec::with_capacity(n_nodes.min(1 << 16));
    for _ in 0..n_nodes {
        let level = cur.u32()?;
        if level as u64 > MAX_LEVEL {
            return Err(malformed("hnsw v1: level exceeds cap"));
        }
        let mut neighbors = Vec::with_capacity((level as usize) + 1);
        for _ in 0..=level {
            let k = cur.u32()? as usize;
            if k as u64 > MAX_DEGREE {
                return Err(malformed("hnsw v1: degree exceeds cap"));
            }
            let mut ids = Vec::with_capacity(k);
            for _ in 0..k {
                ids.push(cur.u32()?);
            }
            neighbors.push(ids);
        }
        nodes.push(Node { level, neighbors });
    }
    Ok(nodes)
}

/// v2: three `intpack` columns (levels, per-layer counts, neighbour ids).
fn decode_nodes_v2(cur: &mut ByteCursor, n_nodes: usize) -> Result<Vec<Node>, RuntimeError> {
    let levels = cur.intpack_column()?;
    if levels.len() != n_nodes {
        return Err(malformed("hnsw v2: level count mismatch"));
    }
    check_cap(&levels, MAX_LEVEL, "level")?;
    // safe: each level <= MAX_LEVEL and levels.len() is bounded by the
    // physically present column, so the sum cannot overflow usize.
    let total_counts: usize = levels.iter().map(|l| *l as usize + 1).sum();
    let counts = cur.intpack_column()?;
    if counts.len() != total_counts {
        return Err(malformed("hnsw v2: layer-count column mismatch"));
    }
    check_cap(&counts, MAX_DEGREE, "degree")?;
    let total_ids: usize = counts.iter().map(|c| *c as usize).sum();
    let ids = cur.intpack_column()?;
    if ids.len() != total_ids {
        return Err(malformed("hnsw v2: neighbour-id column mismatch"));
    }
    let mut nodes = Vec::with_capacity(n_nodes.min(1 << 16));
    let (mut ci, mut ii) = (0usize, 0usize);
    for &lvl in &levels {
        let level = lvl as u32;
        let mut neighbors = Vec::with_capacity(level as usize + 1);
        for _ in 0..=level {
            let k = counts[ci] as usize;
            ci += 1;
            let mut layer = Vec::with_capacity(k);
            for _ in 0..k {
                layer.push(ids[ii] as u32);
                ii += 1;
            }
            neighbors.push(layer);
        }
        nodes.push(Node { level, neighbors });
    }
    Ok(nodes)
}

/// lLight cursor for parsing the on-disk HNSW payload.
struct ByteCursor<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> ByteCursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn u32(&mut self) -> Result<u32, RuntimeError> {
        if self.pos + 4 > self.buf.len() {
            return Err(malformed("hnsw: unexpected EOF"));
        }
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    /// read a length-prefixed `intpack` blob and decode it to `u64`s.
    fn intpack_column(&mut self) -> Result<Vec<u64>, RuntimeError> {
        let len = self.u32()? as usize;
        if self.pos + len > self.buf.len() {
            return Err(malformed("hnsw: truncated intpack column"));
        }
        let blob = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        unpack_u64s(blob).map_err(RuntimeError::Format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_blob(out: &mut Vec<u8>, blob: &[u8]) {
        out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        out.extend_from_slice(blob);
    }

    fn v2_header(n_nodes: u32) -> Vec<u8> {
        let mut out = 2u32.to_le_bytes().to_vec(); // version
        for v in [16u32, 32, 400, 0, 0, n_nodes] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    // a crafted payload must yield a typed Err, never a panic or a giant
    // allocation: the column-length checks alone do not bound the decoded
    // level/degree VALUES, the per-value caps do.
    #[test]
    fn v2_rejects_hostile_level_value() {
        let mut p = v2_header(1);
        write_blob(&mut p, &pack_u64s(&[u64::MAX])); // level = u64::MAX
        write_blob(&mut p, &pack_u64s(&[]));
        write_blob(&mut p, &pack_u64s(&[]));
        assert!(HnswIndex::from_bytes(&p, 1, 8).is_err());
    }

    #[test]
    fn v2_rejects_hostile_degree_value() {
        let mut p = v2_header(1);
        write_blob(&mut p, &pack_u64s(&[0])); // one node, one layer
        write_blob(&mut p, &pack_u64s(&[u64::MAX])); // degree = u64::MAX
        write_blob(&mut p, &pack_u64s(&[]));
        assert!(HnswIndex::from_bytes(&p, 1, 8).is_err());
    }

    #[test]
    fn v2_truncated_columns_error_not_panic() {
        let mut p = v2_header(1);
        write_blob(&mut p, &pack_u64s(&[0]));
        // counts and ids columns missing -> EOF, typed error.
        assert!(HnswIndex::from_bytes(&p, 1, 8).is_err());
    }
}
