//! `NestFile` PyO3 class + `SearchHitPy` data type. Wraps
//! `MmapNestFile` and exposes search/inspect/validate to Python.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass]
pub struct NestFile {
    pub(super) rt: nest_runtime::MmapNestFile,
}

#[pymethods]
impl NestFile {
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let rt = nest_runtime::MmapNestFile::open(std::path::Path::new(path))
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(Self { rt })
    }

    fn search(&self, query: &Bound<PyAny>, k: i32) -> PyResult<Vec<SearchHitPy>> {
        let qvec: Vec<f32> = query
            .extract()
            .map_err(|e| PyValueError::new_err(format!("invalid query vector: {}", e)))?;
        let res = self
            .rt
            .search(&qvec, k)
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(res.hits.into_iter().map(SearchHitPy::from).collect())
    }

    /// lHNSW ANN search with exact rerank. Falls back to `search()` if
    /// the file has no HNSW section.
    fn search_ann(&self, query: &Bound<PyAny>, k: i32, ef: usize) -> PyResult<Vec<SearchHitPy>> {
        let qvec: Vec<f32> = query
            .extract()
            .map_err(|e| PyValueError::new_err(format!("invalid query vector: {}", e)))?;
        let res = self
            .rt
            .search_ann(&qvec, k, ef)
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(res.hits.into_iter().map(SearchHitPy::from).collect())
    }

    /// lGraph search (exact top-ef seed -> bounded bfs over the chunk graph
    /// -> exact rerank on the union). Falls back to `search()` when no
    /// graph_adjacency section is present. The graph only generates
    /// candidates; the returned score is real cosine.
    #[pyo3(signature = (query, k, hops=1, ef=100))]
    fn search_graph(
        &self,
        query: &Bound<PyAny>,
        k: i32,
        hops: usize,
        ef: usize,
    ) -> PyResult<Vec<SearchHitPy>> {
        let qvec: Vec<f32> = query
            .extract()
            .map_err(|e| PyValueError::new_err(format!("invalid query vector: {}", e)))?;
        let res = self
            .rt
            .search_graph(&qvec, k, hops, ef)
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(res.hits.into_iter().map(SearchHitPy::from).collect())
    }

    /// lHybrid (BM25 ∪ vector → exact rerank). Falls back to `search()`
    /// when no BM25 section is present.
    fn search_hybrid(
        &self,
        query: &Bound<PyAny>,
        query_text: &str,
        k: i32,
        candidates: usize,
    ) -> PyResult<Vec<SearchHitPy>> {
        let qvec: Vec<f32> = query
            .extract()
            .map_err(|e| PyValueError::new_err(format!("invalid query vector: {}", e)))?;
        let res = self
            .rt
            .search_hybrid(&qvec, query_text, k, candidates)
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(res.hits.into_iter().map(SearchHitPy::from).collect())
    }

    /// lMetadata-scoped exact search: restrict the exact cosine to the chunks
    /// whose `field` == `value` (via the 0x17 meta_index), then rank that
    /// subset. The score IS the real cosine; recall is 1.0 WITHIN the filter
    /// (exact over the subset). Returns [] when the file has no meta_index or
    /// the (field, value) pair is absent. `field`/`value` are whatever labels
    /// the corpus was built with — no market rule lives in nest.
    fn search_filtered(
        &self,
        query: &Bound<PyAny>,
        field: &str,
        value: &str,
        k: i32,
    ) -> PyResult<Vec<SearchHitPy>> {
        let qvec: Vec<f32> = query
            .extract()
            .map_err(|e| PyValueError::new_err(format!("invalid query vector: {}", e)))?;
        let res = self
            .rt
            .search_filtered(&qvec, field, value, k)
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(res.hits.into_iter().map(SearchHitPy::from).collect())
    }

    /// lAgent-native flagship: a pre-embedded query in, cited spans out.
    /// each hit's `score` IS the exact-cosine rerank value; routes by
    /// manifest capability (hnsw/hybrid/graph/exact). every hit carries the
    /// tier-1 stored canonical `text`, the verifying hashes, the stable
    /// citation_id, and the rerank-source precision marker. embed the query
    /// OFFLINE first (see python/forge/retrieve.py for the potion path).
    #[pyo3(signature = (query, k, candidates=None, hops=1, ef=100))]
    fn retrieve(
        &self,
        query: &Bound<PyAny>,
        k: i32,
        candidates: Option<usize>,
        hops: usize,
        ef: usize,
    ) -> PyResult<Vec<crate::retrieve_fn::RetrieveHitPy>> {
        crate::retrieve_fn::retrieve(&self.rt, query, k, candidates, hops, ef)
    }

    #[getter]
    fn embedding_dim(&self) -> usize {
        self.rt.embedding_dim()
    }

    #[getter]
    fn n_embeddings(&self) -> usize {
        self.rt.n_embeddings()
    }

    #[getter]
    fn dtype(&self) -> &'static str {
        self.rt.dtype().name()
    }

    #[getter]
    fn simd_backend(&self) -> &'static str {
        self.rt.simd_backend().name()
    }

    #[getter]
    fn has_ann(&self) -> bool {
        self.rt.has_ann()
    }

    #[getter]
    fn has_bm25(&self) -> bool {
        self.rt.has_bm25()
    }

    #[getter]
    fn has_graph(&self) -> bool {
        self.rt.has_graph()
    }

    #[getter]
    fn has_meta_index(&self) -> bool {
        self.rt.has_meta_index()
    }

    /// lThe distinct meta_index field names (sorted), or [] when absent.
    fn meta_index_fields(&self) -> Vec<String> {
        self.rt.meta_index_fields()
    }

    #[getter]
    fn file_hash(&self) -> String {
        self.rt.file_hash().to_string()
    }

    #[getter]
    fn content_hash(&self) -> String {
        self.rt.content_hash().to_string()
    }

    /// lMirror of `nest inspect`: returns a Python dict with header,
    /// section table, manifest and hashes.
    fn inspect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let s = self
            .rt
            .inspect_json()
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        py.import("json")?.call_method1("loads", (s,))
    }

    /// lRe-run reader-side validation. Returns `True` on success and
    /// raises `ValueError` (with the reader's typed error in the
    /// message) on any failure.
    fn validate(&self) -> PyResult<bool> {
        self.rt
            .revalidate()
            .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(true)
    }
}

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct SearchHitPy {
    #[pyo3(get)]
    pub chunk_id: String,
    #[pyo3(get)]
    pub score: f32,
    #[pyo3(get)]
    pub score_type: String,
    #[pyo3(get)]
    pub source_uri: String,
    #[pyo3(get)]
    pub offset_start: u64,
    #[pyo3(get)]
    pub offset_end: u64,
    #[pyo3(get)]
    pub embedding_model: String,
    #[pyo3(get)]
    pub index_type: String,
    #[pyo3(get)]
    pub reranked: bool,
    #[pyo3(get)]
    pub file_hash: String,
    #[pyo3(get)]
    pub content_hash: String,
    #[pyo3(get)]
    pub citation_id: String,
}

impl From<nest_runtime::SearchHit> for SearchHitPy {
    fn from(h: nest_runtime::SearchHit) -> Self {
        Self {
            chunk_id: h.chunk_id,
            score: h.score,
            score_type: h.score_type.to_string(),
            source_uri: h.source_uri,
            offset_start: h.offset_start,
            offset_end: h.offset_end,
            embedding_model: h.embedding_model,
            index_type: h.index_type.to_string(),
            reranked: h.reranked,
            file_hash: h.file_hash,
            content_hash: h.content_hash,
            citation_id: h.citation_id,
        }
    }
}
