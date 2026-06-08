//! `search_filtered`: metadata-scoped exact search. In its own module so
//! `search.rs` stays under the 300-line crate guard. It reuses the same
//! pub(crate) rerank helpers (`validate_query`, `score_subset`,
//! `materialize_hits`) as every other path, so its returned `score` is the
//! identical real cosine — only the candidate set differs.

use crate::mmap_file::MmapNestFile;
use crate::search::sort_scores_desc;
use crate::{RuntimeError, SearchExplain, SearchResult};

impl MmapNestFile {
    /// lMetadata-scoped exact search: restrict the exact cosine to the chunks
    /// whose `field` equals `value` (via the 0x17 meta_index), then rank that
    /// subset. The returned `score` IS the real cosine — `score_subset` routes
    /// through the SAME rerank source as every other path — and recall is 1.0
    /// WITHIN the filter: the candidate set IS every chunk matching the value,
    /// scored exactly, no approximation. Returns no hits when the file has no
    /// meta_index or the `(field, value)` pair is absent (an honest empty
    /// result, never a silent fallback to the whole corpus). NO market rule
    /// lives here: `field`/`value` are whatever labels the builder indexed.
    pub fn search_filtered(
        &self,
        query: &[f32],
        field: &str,
        value: &str,
        k: i32,
    ) -> Result<SearchResult, RuntimeError> {
        let t0 = std::time::Instant::now();
        let qnorm = self.validate_query(query, k)?;
        let idxs: Vec<usize> = self
            .meta_index()
            .and_then(|m| m.posting(field, value))
            .map(|p| {
                // ldefense in depth: open() already rejects out-of-range
                // ordinals, but never feed an out-of-bounds index to the rerank,
                // which slices the embedding slab by ordinal unchecked.
                p.iter()
                    .map(|&i| i as usize)
                    .filter(|&i| i < self.n_embeddings)
                    .collect()
            })
            .unwrap_or_default();
        let subset = idxs.len();
        let mut scored = self.score_subset(&qnorm, &idxs)?;
        sort_scores_desc(&mut scored);
        let k_usize = k as usize;
        let truncated = k_usize < subset;
        let top = &scored[..k_usize.min(scored.len())];
        let hits = self.materialize_hits(top, "filtered", false);
        Ok(SearchResult {
            hits: hits.clone(),
            query_time_ms: t0.elapsed().as_secs_f64() * 1000.0,
            index_type: "filtered",
            // lrecall is 1.0 WITHIN the filter (exact over the subset). on an
            // empty candidate set (no meta_index, or the (field,value) matched
            // nothing) recall of nothing is undefined -> NaN, which the printer
            // renders as "not computed" rather than a vacuous 1.0.
            recall: if subset == 0 { f32::NAN } else { 1.0 },
            truncated,
            k_requested: k,
            k_returned: hits.len(),
            explain: SearchExplain::filtered(subset, self.rerank_source_kind()),
        })
    }
}
