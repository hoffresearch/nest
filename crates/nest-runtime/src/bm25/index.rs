//! `Bm25Index` struct + build/search. Encoding/decoding lives in
//! `super::codec`.

use std::collections::HashMap;

use super::tokenize::tokenize;

/// on-disk payload version for the bm25 section (`0x08`). v1 stored every
/// posting as raw `(u32 doc, u32 tf)`; v2 delta-gaps the (sorted) doc ids
/// and bitpacks the gaps, the term-frequencies, and the doc lengths with
/// `intpack`. the decoded index is identical, so scores are unchanged. the
/// reader still accepts v1. the section is optional and excluded from
/// content_hash, so this bump is additive within v1.
pub const BM25_PAYLOAD_VERSION: u32 = 2;
pub const DEFAULT_K1: f32 = 1.5;
pub const DEFAULT_B: f32 = 0.75;

#[derive(Clone, Debug)]
pub(super) struct Posting {
    pub doc: u32,
    pub tf: u32,
}

#[derive(Clone, Debug)]
pub(super) struct TermEntry {
    pub df: u32,
    pub postings: Vec<Posting>,
}

pub struct Bm25Index {
    pub k1: f32,
    pub b: f32,
    pub avgdl: f32,
    pub n_docs: usize,
    pub n_terms: usize,
    pub(super) doc_lengths: Vec<u32>,
    /// term -> entry. HashMap for O(1) query lookup.
    pub(super) terms: HashMap<String, TermEntry>,
}

impl Bm25Index {
    /// lBuild a BM25 index from the canonical chunk texts.
    pub fn build(docs: &[String], k1: f32, b: f32) -> Self {
        let n_docs = docs.len();
        let mut doc_lengths = Vec::with_capacity(n_docs);
        let mut term_postings: HashMap<String, Vec<Posting>> = HashMap::new();
        for (doc_id, doc) in docs.iter().enumerate() {
            let tokens = tokenize(doc);
            doc_lengths.push(tokens.len() as u32);
            let mut tf_map: HashMap<&str, u32> = HashMap::new();
            for t in &tokens {
                *tf_map.entry(t.as_str()).or_insert(0) += 1;
            }
            for (term, tf) in tf_map {
                term_postings
                    .entry(term.to_string())
                    .or_default()
                    .push(Posting {
                        doc: doc_id as u32,
                        tf,
                    });
            }
        }
        let total_dl: u64 = doc_lengths.iter().map(|x| *x as u64).sum();
        let avgdl = if n_docs == 0 {
            0.0
        } else {
            total_dl as f32 / n_docs as f32
        };
        // lSort terms alphabetically so the on-disk encoding is reproducible.
        let mut keys: Vec<String> = term_postings.keys().cloned().collect();
        keys.sort();
        let mut terms: HashMap<String, TermEntry> = HashMap::with_capacity(keys.len());
        for key in keys {
            let mut postings = term_postings.remove(&key).unwrap();
            postings.sort_by_key(|p| p.doc);
            terms.insert(
                key,
                TermEntry {
                    df: postings.len() as u32,
                    postings,
                },
            );
        }
        let n_terms = terms.len();
        Self {
            k1,
            b,
            avgdl,
            n_docs,
            n_terms,
            doc_lengths,
            terms,
        }
    }

    /// lScore `query_text` against the corpus, return the top-k `(doc, score)`
    /// pairs. Empty query returns an empty vec.
    pub fn search(&self, query_text: &str, k: usize) -> Vec<(usize, f32)> {
        if k == 0 || self.n_docs == 0 {
            return Vec::new();
        }
        let q_tokens = tokenize(query_text);
        if q_tokens.is_empty() {
            return Vec::new();
        }
        let mut scores: HashMap<u32, f32> = HashMap::new();
        for term in q_tokens {
            let Some(entry) = self.terms.get(&term) else {
                continue;
            };
            let idf =
                ((self.n_docs as f32 - entry.df as f32 + 0.5) / (entry.df as f32 + 0.5) + 1.0).ln();
            for p in &entry.postings {
                let dl = self.doc_lengths[p.doc as usize] as f32;
                let denom =
                    p.tf as f32 + self.k1 * (1.0 - self.b + self.b * dl / self.avgdl.max(1.0));
                let term_score = idf * (p.tf as f32 * (self.k1 + 1.0)) / denom;
                *scores.entry(p.doc).or_insert(0.0) += term_score;
            }
        }
        let mut all: Vec<(usize, f32)> = scores.into_iter().map(|(d, s)| (d as usize, s)).collect();
        all.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        all.truncate(k);
        all
    }
}
