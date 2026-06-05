//! mmap-backed runtime for `.nest` files.
//!
//! Owns the mmap so `MmapNestFile` is `'static`. Section metadata is parsed
//! once at open time using `NestView`, then the mmap is moved into `Self`
//! and embeddings are read directly from `&self._mmap[offset..]`.
//!
//! Supports float32 / float16 / int8 dtypes with a SIMD dispatcher
//! (AVX2 / NEON / scalar). Optional ANN (`hnsw`) and lexical (`bm25`)
//! sections rerank into the exact cosine path so the final score is
//! always the real cosine value.

pub mod ann;
pub mod bm25;
pub mod error;
mod materialize;
mod mmap_file;
mod rerank;
mod search;
pub mod simd;

pub use error::RuntimeError;
pub use mmap_file::{DType, MmapNestFile};
pub use simd::SimdBackend;

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub score_type: &'static str,
    pub source_uri: String,
    pub offset_start: u64,
    pub offset_end: u64,
    pub embedding_model: String,
    pub index_type: &'static str,
    pub reranked: bool,
    pub file_hash: String,
    pub content_hash: String,
    pub citation_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub query_time_ms: f64,
    pub index_type: &'static str,
    pub recall: f32,
    pub truncated: bool,
    pub k_requested: i32,
    pub k_returned: usize,
}
