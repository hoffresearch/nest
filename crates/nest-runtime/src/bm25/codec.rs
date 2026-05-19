//! On-disk encoding/decoding for the BM25 section (`0x08`). Layout
//! documented in `super::mod`. Term order: alphabetical (deterministic
//! across builds).

use std::collections::HashMap;

use nest_format::error::NestError;

use super::index::{BM25_PAYLOAD_VERSION, Bm25Index, Posting, TermEntry};
use crate::error::RuntimeError;

impl Bm25Index {
    /// lEncode for storage in section `0x08`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&BM25_PAYLOAD_VERSION.to_le_bytes());
        out.extend_from_slice(&self.k1.to_le_bytes());
        out.extend_from_slice(&self.b.to_le_bytes());
        out.extend_from_slice(&self.avgdl.to_le_bytes());
        out.extend_from_slice(&(self.n_docs as u32).to_le_bytes());
        out.extend_from_slice(&(self.n_terms as u32).to_le_bytes());
        for &dl in &self.doc_lengths {
            out.extend_from_slice(&dl.to_le_bytes());
        }
        // lSorted by term for determinism.
        let mut terms: Vec<(&String, &TermEntry)> = self.terms.iter().collect();
        terms.sort_by(|a, b| a.0.cmp(b.0));
        for (term, entry) in terms {
            let bs = term.as_bytes();
            out.extend_from_slice(&(bs.len() as u32).to_le_bytes());
            out.extend_from_slice(bs);
            out.extend_from_slice(&entry.df.to_le_bytes());
            for p in &entry.postings {
                out.extend_from_slice(&p.doc.to_le_bytes());
                out.extend_from_slice(&p.tf.to_le_bytes());
            }
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RuntimeError> {
        let mut cur = ByteCursor::new(bytes);
        let version = cur.u32()?;
        if version != BM25_PAYLOAD_VERSION {
            return Err(RuntimeError::Format(NestError::UnsupportedSectionVersion {
                section_id: nest_format::layout::SECTION_BM25_INDEX,
                version,
            }));
        }
        let k1 = cur.f32()?;
        let b = cur.f32()?;
        let avgdl = cur.f32()?;
        let n_docs = cur.u32()? as usize;
        let n_terms = cur.u32()? as usize;
        let mut doc_lengths = Vec::with_capacity(n_docs);
        for _ in 0..n_docs {
            doc_lengths.push(cur.u32()?);
        }
        let mut terms: HashMap<String, TermEntry> = HashMap::with_capacity(n_terms);
        for _ in 0..n_terms {
            let len = cur.u32()? as usize;
            let term_bytes = cur.bytes(len)?;
            let term = std::str::from_utf8(term_bytes)
                .map_err(|e| {
                    RuntimeError::Format(NestError::MalformedSectionPayload {
                        section_id: nest_format::layout::SECTION_BM25_INDEX,
                        reason: format!("term utf-8: {}", e),
                    })
                })?
                .to_string();
            let df = cur.u32()?;
            let mut postings = Vec::with_capacity(df as usize);
            for _ in 0..df {
                let doc = cur.u32()?;
                let tf = cur.u32()?;
                postings.push(Posting { doc, tf });
            }
            terms.insert(term, TermEntry { df, postings });
        }
        Ok(Self {
            k1,
            b,
            avgdl,
            n_docs,
            n_terms,
            doc_lengths,
            terms,
        })
    }
}

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
                section_id: nest_format::layout::SECTION_BM25_INDEX,
                reason: "unexpected EOF (u32)".into(),
            }));
        }
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, RuntimeError> {
        if self.pos + 4 > self.buf.len() {
            return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                section_id: nest_format::layout::SECTION_BM25_INDEX,
                reason: "unexpected EOF (f32)".into(),
            }));
        }
        let v = f32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], RuntimeError> {
        if self.pos + n > self.buf.len() {
            return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                section_id: nest_format::layout::SECTION_BM25_INDEX,
                reason: format!("unexpected EOF (bytes {})", n),
            }));
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
}
