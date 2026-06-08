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
    /// lMetadata-scoped exact search: restrict the exact cosine to chunks whose
    /// FIELD == VALUE (via the 0x17 meta_index), then rank that subset. Score is
    /// real cosine; recall 1.0 within the filter. QUERY is a JSON array vector
    /// (like `search`). Empty result if the file has no meta_index / no match.
    SearchFiltered {
        file: PathBuf,
        query: String,
        field: String,
        value: String,
        #[arg(short, long, default_value = "10")]
        k: i32,
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
    /// lFlagship verb: text query in, cited answer out. embeds the query
    /// OFFLINE with the default potion static table (never
    /// sentence-transformers), validates model_hash against the manifest,
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
        /// loverride the offline embedder. default: python/forge/embed_query_potion.py.
        #[arg(long)]
        embedder: Option<PathBuf>,
        /// `ef` (HNSW) / candidates-per-path (hybrid). Default: 4*k or 64.
        #[arg(long)]
        candidates: Option<usize>,
        /// llocal path to the vendored potion table dir (fully offline).
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    /// lAgent-shaped flagship: text query in, a json/jsonl answer-pack of
    /// cited spans out. each hit's `score` IS the exact-cosine rerank value.
    /// embeds OFFLINE with the potion table + the same model_hash gate as
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
        /// loverride the offline embedder. default: python/forge/embed_query_potion.py.
        #[arg(long)]
        embedder: Option<PathBuf>,
        #[arg(long)]
        candidates: Option<usize>,
        /// llocal path to the vendored potion table dir (fully offline).
        #[arg(long)]
        model_path: Option<PathBuf>,
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
        Commands::SearchGraph {
            file,
            query,
            k,
            hops,
            ef,
        } => cmd::search_graph::run(file, query, k, hops, ef),
        Commands::SearchFiltered {
            file,
            query,
            field,
            value,
            k,
        } => cmd::search_filtered::run(file, query, field, value, k),
        Commands::Benchmark {
            file,
            queries,
            k,
            ann,
            madvise_cold,
        } => cmd::benchmark::run(file, queries, k, ann, madvise_cold),
        Commands::Stats { file } => cmd::stats::run(file),
        Commands::Cite { file, citation } => cmd::cite::run(file, citation),
        Commands::Ask {
            file,
            query,
            k,
            disclose,
            embedder,
            candidates,
            model_path,
        } => cmd::ask::run(file, query, k, disclose, embedder, candidates, model_path),
        Commands::Retrieve {
            file,
            query,
            k,
            format,
            embedder,
            candidates,
            model_path,
        } => cmd::retrieve::run(file, query, k, format, embedder, candidates, model_path),
    }
}
