//! `build()` PyO3 function: emit a `.nest` from pre-embedded chunks.
//! Resolves preset shortcuts ("exact"/"compressed"/"tiny"/"hybrid")
//! into the underlying `EmbeddingDType` + `SectionEncoding` choices,
//! attaches optional HNSW / BM25 indices.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// lBuild a .nest file from already-embedded chunks.
///
/// `chunks` is a list of dicts with keys:
///   - canonical_text: str
///   - source_uri: str
///   - byte_start: int
///   - byte_end: int
///   - embedding: list[float] (length == embedding_dim, L2-normalized)
///
/// lOptional manifest fields (`title`, `version`, `created`, `description`,
/// `authors`, `license`) and a free-form `provenance` dict can be passed
/// as kwargs. `reproducible=True` overrides `created` so two builds with
/// identical inputs produce byte-identical output.
///
/// lPreset / encoding kwargs:
///   - `preset`: one of "exact" (default), "compressed", "tiny", "hybrid",
///     "nano". "nano" is zstd text / int4 / hnsw (the first sub-int8 size
///     lever, ~2x smaller embeddings than tiny at stored precision; requires
///     `embedding_dim` divisible by 64).
///   - `text_encoding`: "raw" | "zstd" (overrides preset)
///   - `dtype`: "float32" | "float16" | "int8" | "int4" (overrides preset;
///     "int4" requires `embedding_dim` divisible by 64)
///   - `with_hnsw`: bool (overrides preset; default per preset)
///   - `with_bm25`: bool (overrides preset; default per preset)
///   - `hnsw_m`, `hnsw_ef_construction`, `hnsw_seed`: HNSW knobs
#[pyfunction]
#[pyo3(signature = (
    output_path,
    embedding_model,
    embedding_dim,
    chunker_version,
    model_hash,
    chunks,
    *,
    title=None,
    version=None,
    created=None,
    description=None,
    authors=None,
    license=None,
    provenance=None,
    reproducible=false,
    preset="exact",
    text_encoding=None,
    dtype=None,
    with_hnsw=None,
    with_bm25=None,
    hnsw_m=16,
    hnsw_ef_construction=400,
    hnsw_seed=42,
))]
#[allow(clippy::too_many_arguments)]
pub fn build(
    py: Python<'_>,
    output_path: &str,
    embedding_model: &str,
    embedding_dim: u32,
    chunker_version: &str,
    model_hash: &str,
    chunks: &Bound<PyList>,
    title: Option<String>,
    version: Option<String>,
    created: Option<String>,
    description: Option<String>,
    authors: Option<Vec<String>>,
    license: Option<String>,
    provenance: Option<&Bound<PyDict>>,
    reproducible: bool,
    preset: &str,
    text_encoding: Option<&str>,
    dtype: Option<&str>,
    with_hnsw: Option<bool>,
    with_bm25: Option<bool>,
    hnsw_m: usize,
    hnsw_ef_construction: usize,
    hnsw_seed: u64,
) -> PyResult<String> {
    use nest_format::manifest::Manifest;
    use nest_format::writer::{EmbeddingDType, NestFileBuilder, SectionEncoding};

    let n_chunks = chunks.len() as u64;
    let chunk_inputs = parse_chunks(chunks)?;

    let provenance_value = match provenance {
        Some(p) => {
            let s: String = py.import("json")?.call_method1("dumps", (p,))?.extract()?;
            serde_json::from_str(&s)
                .map_err(|e| PyValueError::new_err(format!("provenance JSON: {}", e)))?
        }
        None => serde_json::json!({}),
    };

    // lResolve preset defaults; explicit kwargs win. "nano" sits below
    // "tiny": int4 block-64 embeddings at stored precision (~2x over int8),
    // zstd text, hnsw shortlist. exact/compressed/tiny/hybrid are byte-
    // frozen and unchanged.
    let (default_text_enc, default_dtype, default_hnsw, default_bm25) = match preset {
        "exact" => (SectionEncoding::Raw, EmbeddingDType::Float32, false, false),
        "compressed" => (SectionEncoding::Zstd, EmbeddingDType::Float16, false, false),
        "tiny" => (SectionEncoding::Zstd, EmbeddingDType::Int8, true, false),
        "nano" => (SectionEncoding::Zstd, EmbeddingDType::Int4, true, false),
        "hybrid" => (SectionEncoding::Zstd, EmbeddingDType::Float32, true, true),
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown preset: {} (expected exact|compressed|tiny|nano|hybrid)",
                other
            )));
        }
    };
    let text_enc = match text_encoding {
        Some("raw") => SectionEncoding::Raw,
        Some("zstd") => SectionEncoding::Zstd,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "unknown text_encoding: {} (expected raw|zstd)",
                other
            )));
        }
        None => default_text_enc,
    };
    let dt = match dtype {
        Some("float32") => EmbeddingDType::Float32,
        Some("float16") => EmbeddingDType::Float16,
        Some("int8") => EmbeddingDType::Int8,
        Some("int4") => EmbeddingDType::Int4,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "unknown dtype: {} (expected float32|float16|int8|int4)",
                other
            )));
        }
        None => default_dtype,
    };
    // lint4 requires the embedding_dim to be a multiple of the block size so
    // every 64-dim group has its own absmax scale. Fail fast with a clear
    // message rather than deep in the encoder.
    if dt == EmbeddingDType::Int4 && (embedding_dim == 0 || embedding_dim % 64 != 0) {
        return Err(PyValueError::new_err(format!(
            "int4 requires embedding_dim divisible by 64, got {}",
            embedding_dim
        )));
    }
    let want_hnsw = with_hnsw.unwrap_or(default_hnsw);
    let want_bm25 = with_bm25.unwrap_or(default_bm25);

    let manifest = Manifest {
        embedding_model: embedding_model.to_string(),
        embedding_dim,
        n_chunks,
        chunker_version: chunker_version.to_string(),
        model_hash: model_hash.to_string(),
        title,
        version,
        created,
        description,
        authors,
        license,
        ..Default::default()
    };

    let mut builder = NestFileBuilder::new(manifest)
        .reproducible(reproducible)
        .with_provenance(provenance_value)
        .text_encoding(text_enc)
        .embedding_dtype(dt);

    // lHNSW: build the index from f32 vectors (we have them in chunk_inputs
    // already). The runtime materializes f32 vectors at open time too —
    // here we use the originals so build is independent of dtype loss.
    if want_hnsw {
        let dim = embedding_dim as usize;
        let n = chunk_inputs.len();
        let mut flat: Vec<f32> = Vec::with_capacity(n * dim);
        for c in &chunk_inputs {
            flat.extend_from_slice(&c.embedding);
        }
        let idx = nest_runtime::ann::HnswIndex::build(
            flat,
            n,
            dim,
            hnsw_m,
            hnsw_ef_construction,
            hnsw_seed,
        );
        builder = builder.hnsw_index(idx.to_bytes());
    }

    if want_bm25 {
        let docs: Vec<String> = chunk_inputs
            .iter()
            .map(|c| c.canonical_text.clone())
            .collect();
        let bm = nest_runtime::bm25::Bm25Index::build(
            &docs,
            nest_runtime::bm25::DEFAULT_K1,
            nest_runtime::bm25::DEFAULT_B,
        );
        builder = builder.bm25_index(bm.to_bytes());
    }

    builder = builder.add_chunks(chunk_inputs);

    let bytes = builder
        .build_bytes()
        .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    std::fs::write(output_path, &bytes)
        .map_err(|e| PyValueError::new_err(format!("write {}: {}", output_path, e)))?;
    Ok(output_path.to_string())
}

fn parse_chunks(chunks: &Bound<PyList>) -> PyResult<Vec<nest_format::ChunkInput>> {
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
