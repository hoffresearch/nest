//! On-disk encoding/decoding for the BM25 section (`0x08`). Term order:
//! alphabetical (deterministic across builds).
//!
//! Payload version 2 bitpacks the integer-heavy parts with the shared
//! `intpack` codec: the doc lengths, the delta-gapped (sorted) doc ids,
//! and the term frequencies each become one `intpack` column. The decoded
//! index is identical to the built one, so scores are unchanged. Version 1
//! (flat `u32` postings) is still accepted on read. The section is optional
//! and excluded from content_hash.

use std::collections::HashMap;

use nest_format::encoding::{pack_u64s, unpack_u64s};
use nest_format::error::NestError;

use super::index::{BM25_PAYLOAD_VERSION, Bm25Index, Posting, TermEntry};
use crate::error::RuntimeError;

impl Bm25Index {
    /// lEncode for storage in section `0x08` (v2).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&BM25_PAYLOAD_VERSION.to_le_bytes());
        out.extend_from_slice(&self.k1.to_le_bytes());
        out.extend_from_slice(&self.b.to_le_bytes());
        out.extend_from_slice(&self.avgdl.to_le_bytes());
        out.extend_from_slice(&(self.n_docs as u32).to_le_bytes());
        out.extend_from_slice(&(self.n_terms as u32).to_le_bytes());

        let dls: Vec<u64> = self.doc_lengths.iter().map(|&x| x as u64).collect();
        write_blob(&mut out, &pack_u64s(&dls));

        // lSorted by term for determinism.
        let mut terms: Vec<(&String, &TermEntry)> = self.terms.iter().collect();
        terms.sort_by(|a, b| a.0.cmp(b.0));
        let mut doc_gaps: Vec<u64> = Vec::new();
        let mut tfs: Vec<u64> = Vec::new();
        for (term, entry) in &terms {
            let bs = term.as_bytes();
            out.extend_from_slice(&(bs.len() as u32).to_le_bytes());
            out.extend_from_slice(bs);
            out.extend_from_slice(&entry.df.to_le_bytes());
            let mut prev = 0u32;
            for p in &entry.postings {
                doc_gaps.push(p.doc.wrapping_sub(prev) as u64);
                prev = p.doc;
                tfs.push(p.tf as u64);
            }
        }
        write_blob(&mut out, &pack_u64s(&doc_gaps));
        write_blob(&mut out, &pack_u64s(&tfs));
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RuntimeError> {
        let mut cur = ByteCursor::new(bytes);
        let version = cur.u32()?;
        let k1 = cur.f32()?;
        let b = cur.f32()?;
        let avgdl = cur.f32()?;
        let n_docs = cur.u32()? as usize;
        let n_terms = cur.u32()? as usize;
        let (doc_lengths, terms) = match version {
            1 => decode_v1(&mut cur, n_docs, n_terms)?,
            2 => decode_v2(&mut cur, n_docs, n_terms)?,
            other => {
                return Err(RuntimeError::Format(NestError::UnsupportedSectionVersion {
                    section_id: nest_format::layout::SECTION_BM25_INDEX,
                    version: other,
                }));
            }
        };
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

fn malformed(reason: impl Into<String>) -> RuntimeError {
    RuntimeError::Format(NestError::MalformedSectionPayload {
        section_id: nest_format::layout::SECTION_BM25_INDEX,
        reason: reason.into(),
    })
}

fn write_blob(out: &mut Vec<u8>, blob: &[u8]) {
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(blob);
}

type Decoded = (Vec<u32>, HashMap<String, TermEntry>);

/// v1: flat `u32` doc lengths and `(doc, tf)` postings.
fn decode_v1(cur: &mut ByteCursor, n_docs: usize, n_terms: usize) -> Result<Decoded, RuntimeError> {
    let mut doc_lengths = Vec::with_capacity(n_docs);
    for _ in 0..n_docs {
        doc_lengths.push(cur.u32()?);
    }
    let mut terms: HashMap<String, TermEntry> = HashMap::with_capacity(n_terms);
    for _ in 0..n_terms {
        let term = cur.term()?;
        let df = cur.u32()?;
        let mut postings = Vec::with_capacity(df as usize);
        for _ in 0..df {
            let doc = cur.u32()?;
            let tf = cur.u32()?;
            postings.push(Posting { doc, tf });
        }
        terms.insert(term, TermEntry { df, postings });
    }
    Ok((doc_lengths, terms))
}

/// v2: `intpack` columns for doc lengths, delta-gapped doc ids, and tfs.
fn decode_v2(cur: &mut ByteCursor, n_docs: usize, n_terms: usize) -> Result<Decoded, RuntimeError> {
    let dls = cur.intpack_column()?;
    if dls.len() != n_docs {
        return Err(malformed("bm25 v2: doc-length column mismatch"));
    }
    let doc_lengths: Vec<u32> = dls.iter().map(|&x| x as u32).collect();

    // term dictionary: (term, df) in alphabetical order.
    let mut dict: Vec<(String, u32)> = Vec::with_capacity(n_terms);
    let mut total: usize = 0;
    for _ in 0..n_terms {
        let term = cur.term()?;
        let df = cur.u32()?;
        total += df as usize;
        dict.push((term, df));
    }
    let gaps = cur.intpack_column()?;
    let tfs = cur.intpack_column()?;
    if gaps.len() != total || tfs.len() != total {
        return Err(malformed("bm25 v2: posting column mismatch"));
    }
    let mut terms: HashMap<String, TermEntry> = HashMap::with_capacity(n_terms);
    let mut k = 0usize;
    for (term, df) in dict {
        let mut postings = Vec::with_capacity(df as usize);
        let mut prev = 0u32;
        for _ in 0..df {
            let doc = prev.wrapping_add(gaps[k] as u32);
            prev = doc;
            postings.push(Posting {
                doc,
                tf: tfs[k] as u32,
            });
            k += 1;
        }
        terms.insert(term, TermEntry { df, postings });
    }
    Ok((doc_lengths, terms))
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
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, RuntimeError> {
        let b = self.bytes(4)?;
        Ok(f32::from_le_bytes(b.try_into().unwrap()))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], RuntimeError> {
        if self.pos + n > self.buf.len() {
            return Err(malformed(format!("bm25: unexpected EOF (need {})", n)));
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
    fn term(&mut self) -> Result<String, RuntimeError> {
        let len = self.u32()? as usize;
        let b = self.bytes(len)?;
        std::str::from_utf8(b)
            .map(|s| s.to_string())
            .map_err(|e| malformed(format!("term utf-8: {}", e)))
    }
    fn intpack_column(&mut self) -> Result<Vec<u64>, RuntimeError> {
        let len = self.u32()? as usize;
        let blob = self.bytes(len)?;
        unpack_u64s(blob).map_err(RuntimeError::Format)
    }
}
