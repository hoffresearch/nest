use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cmd;

#[derive(Parser)]
#[command(name = "nest")]
#[command(about = ".nest — Semantic Knowledge Format for Local Agents", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// lInspect file metadata, manifest, and section table.
    Inspect {
        file: PathBuf,
        /// lEmit as JSON instead of the human-readable layout. Schema:
        /// `{magic, version_major, version_minor, format_version,
        /// schema_version, embedding_dim, n_chunks, n_embeddings,
        /// file_size, manifest, sections[], file_hash, content_hash,
        /// simd_backend}`.
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect { file, json } => cmd::inspect::run(file, json),
        Commands::Validate { file } => cmd::validate::run(file),
        Commands::Search { file, query, k } => cmd::search::run(file, query, k),
        Commands::SearchText {
            file,
            query,
            k,
            embedder,
            candidates,
            model_path,
            skip_model_hash_check,
        } => cmd::search_text::run(
            file,
            query,
            k,
            embedder,
            candidates,
            model_path,
            skip_model_hash_check,
        ),
        Commands::SearchAnn { file, query, k, ef } => cmd::search_ann::run(file, query, k, ef),
        Commands::Benchmark {
            file,
            queries,
            k,
            ann,
            madvise_cold,
        } => cmd::benchmark::run(file, queries, k, ann, madvise_cold),
        Commands::Stats { file } => cmd::stats::run(file),
        Commands::Cite { file, citation } => cmd::cite::run(file, citation),
    }
}
