//! The clap surface: `Cli` + `Commands`. Split from `main.rs` (which keeps
//! the dispatch) so both stay under the 300-line crate guard.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::cmd;

#[derive(Parser)]
#[command(name = "nest")]
#[command(version)]
#[command(about = ".nest — Semantic Knowledge Format for Local Agents", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// lInspect file metadata, manifest, and section table.
    Inspect {
        file: PathBuf,
        /// lEmit as JSON instead of the human-readable layout. Schema:
        /// `{magic, version_major, version_minor, format_version,
        /// schema_version, embedding_dim, n_chunks, n_embeddings,
        /// file_size, manifest, sections[], blobs, spaces[], file_hash,
        /// content_hash, simd_backend}`.
        #[arg(long)]
        json: bool,
    },
    /// lValidate file integrity (magic, checksums, hashes, manifest, contract).
    Validate { file: PathBuf },
    /// lSearch a `.nest` file with a JSON-array query vector (exact path).
    Search {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
    },
    /// lSearch by raw text — embeds the query with the model declared in
    /// the manifest, then runs the appropriate vector path. Honors the
    /// declared `index_type` (exact / hnsw / hybrid). Validates the
    /// embedder's model_hash against the manifest before running search;
    /// a mismatch fails with a typed error rather than returning
    /// silently-bad results.
    SearchText {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        /// lOverride the embedder script. Default: `python/embed_query.py`.
        #[arg(long)]
        embedder: Option<PathBuf>,
        /// `ef` (HNSW) / candidates-per-path (hybrid). Default: 4*k or 64.
        #[arg(long)]
        candidates: Option<usize>,
        /// lLocal path to the model snapshot dir. Use this for fully
        /// offline operation: copy the model dir alongside the .nest,
        /// pass --model-path at every search. Without this, the
        /// embedder resolves the model from the sentence-transformers
        /// cache (requires network on first use).
        #[arg(long)]
        model_path: Option<PathBuf>,
        /// lSkip model_hash validation. ONLY use when intentionally
        /// running search-text against a corpus whose `model_hash`
        /// is the legacy zero-placeholder (pre-Phase-3 builds). In
        /// that case the search is still cosine-valid IF the user
        /// genuinely uses the same embedding model — but there is
        /// no guarantee. Prefer rebuilding the corpus.
        #[arg(long)]
        skip_model_hash_check: bool,
    },
    /// lForce the ANN (HNSW) path. Falls back to exact if the file has
    /// no HNSW section.
    SearchAnn {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        #[arg(long, default_value = "100")]
        ef: usize,
    },
    /// lGraph search: seed from the exact-cosine top-`ef`, expand a bounded
    /// bfs over the chunk-to-chunk graph, then exact-rerank the union. The
    /// graph only generates candidates; the score is real cosine. Falls back
    /// to exact if the file has no graph_adjacency section.
    SearchGraph {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        #[arg(long, default_value = "1")]
        hops: usize,
        #[arg(long, default_value = "100")]
        ef: usize,
    },
    /// lExact search over one NAMED multimodal space (0x15 band). The query
    /// vector must be embedded with the space's model and have the space's
    /// dim; mismatches are typed errors, never a silent text-path fallback.
    SearchSpace {
        file: PathBuf,
        /// lJSON array of f32 at the space's dim.
        query: String,
        /// lspace name as listed by `stats` / `inspect --json` (e.g. "wemm-2b@256").
        #[arg(long)]
        space: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        /// lAlso assert the space's model_hash equals this value.
        #[arg(long)]
        expect_model_hash: Option<String>,
    },
    /// lDeclarative corpus build from a TOML/JSON spec (launcher over
    /// python/tools/nest_forge.py; the build is officially a python
    /// frontend). Streams the tool's output and propagates its exit code.
    Build {
        /// lbuild spec path (.toml or .json).
        #[arg(long)]
        spec: PathBuf,
        /// levenly-spaced row subset for pilots.
        #[arg(long)]
        sample: Option<usize>,
        /// lcomma-separated preset subset (e.g. "potion,wemm-2b").
        #[arg(long)]
        models: Option<String>,
        /// loverride the spec's [output].dir.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// lresume from per-stage state after an interrupted build.
        #[arg(long)]
        resume: bool,
        /// lre-emit byte-identically from cached vectors (L3 check).
        #[arg(long)]
        rebuild_only: bool,
        /// lresolve the plan + dependency status without loading models.
        #[arg(long)]
        dry_run: bool,
        /// lallow presets flagged too heavy for this machine (wemm-4b/9b).
        #[arg(long)]
        allow_heavy: bool,
    },
    /// lBenchmark exact flat search latency.
    Benchmark {
        file: PathBuf,
        #[arg(short, long, default_value = "100")]
        queries: usize,
        #[arg(short, long, default_value = "10")]
        k: i32,
        /// lIf set, also benchmark `search_ann` with the given ef.
        #[arg(long)]
        ann: Option<usize>,
        /// lForce a "madvise-cold" cache between queries by calling
        /// posix_madvise(MADV_DONTNEED) on the mmap. Approximates the
        /// first hit pos-boot — but it's a hint, not a guarantee.
        /// lSee MmapNestFile::madvise_cold for caveats.
        #[arg(long)]
        madvise_cold: bool,
        /// lBenchmark the named multimodal space instead of the default path.
        #[arg(long)]
        space: Option<String>,
    },
    /// lShow file stats.
    Stats { file: PathBuf },
    /// lResolve a `nest://content_hash/chunk_id` citation into the
    /// canonical text and original span for the chunk.
    Cite {
        file: PathBuf,
        /// `nest://<content_hash>/<chunk_id>` URI.
        citation: String,
    },
    /// lFlagship verb: text query in, cited answer out. embeds the query
    /// OFFLINE (potion for potion corpora; the registry embedder for any
    /// other manifest model), validates model_hash against the manifest,
    /// routes by manifest capability, and prints the cited canonical text
    /// with a nest:// citation. `--disclose explain` adds the rerank-source
    /// honesty line (real cosine vs real cosine at stored precision). cite is
    /// tier-1: the printed text is the stored canonical text, never an
    /// original-byte reopen.
    Ask {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        /// ldisclosure level: `answer` (cited text + nest:// only, default)
        /// or `explain` (also the rerank-source honesty line + route).
        #[arg(long, value_enum, default_value = "answer")]
        disclose: cmd::ask::Disclose,
        /// loverride the offline embedder. default: routed by manifest model.
        #[arg(long)]
        embedder: Option<PathBuf>,
        /// `ef` (HNSW) / candidates-per-path (hybrid). Default: 4*k or 64.
        #[arg(long)]
        candidates: Option<usize>,
        /// llocal path to the model dir (fully offline).
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    /// lAgent-shaped flagship: text query in, a json/jsonl answer-pack of
    /// cited spans out. each hit's `score` IS the exact-cosine rerank value.
    /// embeds OFFLINE with the same routed embedder + model_hash gate as
    /// `ask`. `text` is the stored canonical text (TIER-1), the citation_id
    /// round-trips through `cite`; never an original-byte reopen.
    Retrieve {
        file: PathBuf,
        query: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
        /// loutput format: `jsonl` (one object per line, default) or `json`.
        #[arg(long, value_enum, default_value = "jsonl")]
        format: cmd::retrieve::Format,
        /// loverride the offline embedder. default: routed by manifest model.
        #[arg(long)]
        embedder: Option<PathBuf>,
        #[arg(long)]
        candidates: Option<usize>,
        /// llocal path to the model dir (fully offline).
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    /// lPost-install health check: versions, simd backend, python deps, and
    /// one real offline potion embed. exits with a typed code (0 ok, 2 python
    /// missing, 3 python deps missing, 4 embedder missing, 5 potion table
    /// missing, 6 embedder run failed).
    Doctor,
}
