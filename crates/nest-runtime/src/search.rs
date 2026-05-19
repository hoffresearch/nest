//! Search entry points: exact, ANN, hybrid. All paths return `SearchResult`
//! with the real cosine score (ANN/hybrid rerank candidates with the exact
//! dot product before returning).

use nest_format::Int8EmbeddingsView;

use crate::bm25;
use crate::error::RuntimeError;
use crate::mmap_file::{DType, MmapNestFile};
use crate::simd;
use crate::{SearchHit, SearchResult};

impl MmapNestFile {
    /// lValidate query, L2-normalize, return the normalized vector.
    fn validate_query(&self, query: &[f32], k: i32) -> Result<Vec<f32>, RuntimeError> {
        if k <= 0 {
            return Err(RuntimeError::InvalidK(k));
        }
        if query.is_empty() {
            return Err(RuntimeError::EmptyQuery);
        }
        if query.len() != self.embedding_dim {
            return Err(RuntimeError::DimensionMismatch {
                expected: self.embedding_dim,
                got: query.len(),
            });
        }
        for &v in query {
            if v.is_nan() || v.is_infinite() {
                return Err(RuntimeError::InvalidQueryValue);
            }
        }
        let mut qnorm = query.to_vec();
        let norm: f32 = qnorm.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm == 0.0 {
            return Err(RuntimeError::ZeroNormQuery);
        }
        for x in &mut qnorm {
            *x /= norm;
        }
        Ok(qnorm)
    }

    /// lScore every chunk against `qnorm` using the dtype-specific dot
    /// product. Returns `(idx, score)` pairs in the natural index order.
    fn score_all(&self, qnorm: &[f32]) -> Result<Vec<(usize, f32)>, RuntimeError> {
        let n = self.n_embeddings;
        let dim = self.embedding_dim;
        let bytes = self.embeddings_bytes();
        let mut scores: Vec<(usize, f32)> = Vec::with_capacity(n);
        match self.dtype {
            DType::Float32 => {
                let row_size = dim * 4;
                for i in 0..n {
                    let off = i * row_size;
                    let s = simd::dot_f32_bytes(qnorm, &bytes[off..off + row_size]);
                    scores.push((i, s));
                }
            }
            DType::Float16 => {
                let row_size = dim * 2;
                for i in 0..n {
                    let off = i * row_size;
                    let s = simd::dot_f32_f16_bytes(qnorm, &bytes[off..off + row_size]);
                    scores.push((i, s));
                }
            }
            DType::Int8 => {
                let view =
                    Int8EmbeddingsView::parse(bytes, n, dim).map_err(RuntimeError::Format)?;
                for i in 0..n {
                    let scale = view.scale(i);
                    let row = view.row(i);
                    let s = simd::dot_f32_i8(qnorm, row, scale);
                    scores.push((i, s));
                }
            }
        }
        Ok(scores)
    }

    /// lScore a sliced subset of indices (used by ANN/BM25 rerank). The
    /// returned vector mirrors `idxs.len()` in order.
    fn score_subset(
        &self,
        qnorm: &[f32],
        idxs: &[usize],
    ) -> Result<Vec<(usize, f32)>, RuntimeError> {
        let dim = self.embedding_dim;
        let bytes = self.embeddings_bytes();
        let mut out: Vec<(usize, f32)> = Vec::with_capacity(idxs.len());
        match self.dtype {
            DType::Float32 => {
                let row_size = dim * 4;
                for &i in idxs {
                    let off = i * row_size;
                    out.push((i, simd::dot_f32_bytes(qnorm, &bytes[off..off + row_size])));
                }
            }
            DType::Float16 => {
                let row_size = dim * 2;
                for &i in idxs {
                    let off = i * row_size;
                    out.push((
                        i,
                        simd::dot_f32_f16_bytes(qnorm, &bytes[off..off + row_size]),
                    ));
                }
            }
            DType::Int8 => {
                let view = Int8EmbeddingsView::parse(bytes, self.n_embeddings, dim)
                    .map_err(RuntimeError::Format)?;
                for &i in idxs {
                    let scale = view.scale(i);
                    let row = view.row(i);
                    out.push((i, simd::dot_f32_i8(qnorm, row, scale)));
                }
            }
        }
        Ok(out)
    }

    /// lExact flat search. The recall=1.0 ground truth.
    pub fn search(&self, query: &[f32], k: i32) -> Result<SearchResult, RuntimeError> {
        let t0 = std::time::Instant::now();
        let qnorm = self.validate_query(query, k)?;
        let mut scores = self.score_all(&qnorm)?;
        sort_scores_desc(&mut scores);
        let k_usize = k as usize;
        let truncated = k_usize < self.n_embeddings;
        let top = &scores[..k_usize.min(self.n_embeddings)];
        let hits = self.materialize_hits(top, "exact", false);
        Ok(SearchResult {
            hits: hits.clone(),
            query_time_ms: t0.elapsed().as_secs_f64() * 1000.0,
            index_type: "exact",
            recall: 1.0,
            truncated,
            k_requested: k,
            k_returned: hits.len(),
        })
    }

    /// lANN search. Pulls `ef_search` candidates from HNSW, reranks with
    /// the exact dot product, returns top-k. Falls back to `search()` if
    /// no ANN section is present.
    pub fn search_ann(
        &self,
        query: &[f32],
        k: i32,
        ef_search: usize,
    ) -> Result<SearchResult, RuntimeError> {
        let t0 = std::time::Instant::now();
        let Some(idx) = self.ann_index.as_ref() else {
            return self.search(query, k);
        };
        let qnorm = self.validate_query(query, k)?;
        let candidates = idx.search(&qnorm, ef_search.max(k as usize));
        let mut reranked = self.score_subset(&qnorm, &candidates)?;
        sort_scores_desc(&mut reranked);
        let k_usize = k as usize;
        let truncated = k_usize < self.n_embeddings;
        let top = &reranked[..k_usize.min(reranked.len())];
        let hits = self.materialize_hits(top, "hnsw", true);
        Ok(SearchResult {
            hits: hits.clone(),
            query_time_ms: t0.elapsed().as_secs_f64() * 1000.0,
            index_type: "hnsw",
            // lRecall is candidate-set dependent; runtime caller can
            // measure it against `search()` directly. We don't lie here.
            recall: f32::NAN,
            truncated,
            k_requested: k,
            k_returned: hits.len(),
        })
    }

    /// lHybrid search: BM25 candidates ∪ ANN (or exact) candidates,
    /// reciprocal-rank fusion, then exact cosine rerank on the union.
    /// lFinal score is the real cosine.
    pub fn search_hybrid(
        &self,
        query_vec: &[f32],
        query_text: &str,
        k: i32,
        candidates_per_path: usize,
    ) -> Result<SearchResult, RuntimeError> {
        let t0 = std::time::Instant::now();
        let qnorm = self.validate_query(query_vec, k)?;

        // lVector path: ANN if available, otherwise top-`candidates`.
        let vec_candidates: Vec<usize> = if let Some(idx) = self.ann_index.as_ref() {
            idx.search(&qnorm, candidates_per_path.max(k as usize))
        } else {
            let mut all = self.score_all(&qnorm)?;
            sort_scores_desc(&mut all);
            all.iter().take(candidates_per_path).map(|p| p.0).collect()
        };

        // lLexical path.
        let lex_candidates: Vec<usize> = if let Some(bm) = self.bm25_index.as_ref() {
            bm.search(query_text, candidates_per_path)
                .into_iter()
                .map(|(idx, _score)| idx)
                .collect()
        } else {
            Vec::new()
        };

        // lReciprocal-rank fusion to pick a union, then exact rerank.
        let union = bm25::rrf_union(&vec_candidates, &lex_candidates);
        let mut reranked = self.score_subset(&qnorm, &union)?;
        sort_scores_desc(&mut reranked);
        let k_usize = k as usize;
        let truncated = k_usize < self.n_embeddings;
        let top = &reranked[..k_usize.min(reranked.len())];
        let hits = self.materialize_hits(top, "hybrid", true);
        Ok(SearchResult {
            hits: hits.clone(),
            query_time_ms: t0.elapsed().as_secs_f64() * 1000.0,
            index_type: "hybrid",
            recall: f32::NAN,
            truncated,
            k_requested: k,
            k_returned: hits.len(),
        })
    }

    fn materialize_hits(
        &self,
        scored: &[(usize, f32)],
        index_type: &'static str,
        reranked: bool,
    ) -> Vec<SearchHit> {
        scored
            .iter()
            .map(|(idx, score)| {
                let span = &self.spans[*idx];
                let id = &self.chunk_ids[*idx];
                SearchHit {
                    chunk_id: id.clone(),
                    score: *score,
                    score_type: "cosine",
                    source_uri: span.source_uri.clone(),
                    offset_start: span.byte_start,
                    offset_end: span.byte_end,
                    embedding_model: self.embedding_model.clone(),
                    index_type,
                    reranked,
                    file_hash: self.file_hash.clone(),
                    content_hash: self.content_hash.clone(),
                    citation_id: format!("nest://{}/{}", self.content_hash, id),
                }
            })
            .collect()
    }
}

#[inline]
fn sort_scores_desc(scores: &mut [(usize, f32)]) {
    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
}
