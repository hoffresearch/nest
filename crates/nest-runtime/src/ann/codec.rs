//! On-disk encoding/decoding for the HNSW section (`0x07`, `encoding=raw`).
//! Layout is documented in `super::mod` and frozen at payload version 1.
//!
//! Section checksum (8 bytes of SHA-256 over the physical bytes) is
//! computed by the writer at file-build time, not here.

use nest_format::error::NestError;

use super::{HNSW_PAYLOAD_VERSION, HnswIndex, Node};
use crate::error::RuntimeError;
use crate::materialize::PackedVectors;

impl HnswIndex {
    /// lEncode the index to bytes for embedding in section `0x07`.
    pub fn to_bytes(&self) -> Vec<u8> {
        // 7 u32 header fields + per-node (level + per-layer (count + ids)).
        let mut out = Vec::with_capacity(7 * 4 + self.n * 16);
        out.extend_from_slice(&HNSW_PAYLOAD_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.m as u32).to_le_bytes());
        out.extend_from_slice(&(self.m_max0 as u32).to_le_bytes());
        out.extend_from_slice(&(self.ef_construction as u32).to_le_bytes());
        out.extend_from_slice(&self.entry_point.to_le_bytes());
        out.extend_from_slice(&self.max_level.to_le_bytes());
        out.extend_from_slice(&(self.n as u32).to_le_bytes());
        for node in &self.nodes {
            out.extend_from_slice(&node.level.to_le_bytes());
            for layer in 0..=node.level {
                let nbrs = &node.neighbors[layer as usize];
                out.extend_from_slice(&(nbrs.len() as u32).to_le_bytes());
                for &id in nbrs {
                    out.extend_from_slice(&id.to_le_bytes());
                }
            }
        }
        out
    }

    /// lParse an HNSW payload from bytes. The vectors are reconstructed
    /// from the embeddings section by the caller; this constructor is
    /// for the on-disk graph only. Call `attach_vectors` before search.
    pub fn from_bytes(bytes: &[u8], n: usize, dim: usize) -> Result<Self, RuntimeError> {
        let mut cur = ByteCursor::new(bytes);
        let version = cur.u32()?;
        if version != HNSW_PAYLOAD_VERSION {
            return Err(RuntimeError::Format(NestError::UnsupportedSectionVersion {
                section_id: nest_format::layout::SECTION_HNSW_INDEX,
                version,
            }));
        }
        let m = cur.u32()? as usize;
        let m_max0 = cur.u32()? as usize;
        let ef_construction = cur.u32()? as usize;
        let entry_point = cur.u32()?;
        let max_level = cur.u32()?;
        let n_nodes = cur.u32()? as usize;
        if n_nodes != n {
            return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                section_id: nest_format::layout::SECTION_HNSW_INDEX,
                reason: format!("node count {} != n_embeddings {}", n_nodes, n),
            }));
        }
        let mut nodes = Vec::with_capacity(n_nodes);
        for _ in 0..n_nodes {
            let level = cur.u32()?;
            let mut neighbors = Vec::with_capacity((level as usize) + 1);
            for _ in 0..=level {
                let k = cur.u32()? as usize;
                let mut ids = Vec::with_capacity(k);
                for _ in 0..k {
                    ids.push(cur.u32()?);
                }
                neighbors.push(ids);
            }
            nodes.push(Node { level, neighbors });
        }
        // lThe reader must attach vectors before we can search; until then
        // the store is empty. The runtime sets it via attach_store.
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
            return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                section_id: nest_format::layout::SECTION_HNSW_INDEX,
                reason: "unexpected EOF".into(),
            }));
        }
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
}
