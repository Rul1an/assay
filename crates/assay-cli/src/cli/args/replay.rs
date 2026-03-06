use clap::{Parser, Subcommand};
use std::path::PathBuf;

use super::ValidateOutputFormat;

#[derive(clap::Args, Debug, Clone)]
pub struct ValidateArgs {
    #[arg(long, default_value = "assay.yaml")]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub trace_file: Option<std::path::PathBuf>,

    #[arg(long)]
    pub baseline: Option<std::path::PathBuf>,

    #[arg(long, default_value = "false")]
    pub replay_strict: bool,

    #[arg(long, value_enum, default_value_t = ValidateOutputFormat::Text)]
    pub format: ValidateOutputFormat,

    #[arg(long)]
    pub output: Option<std::path::PathBuf>,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,
}

#[derive(Parser, Clone)]
pub struct QuarantineArgs {
    #[command(subcommand)]
    pub cmd: QuarantineSub,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,
    #[arg(long, default_value = "demo")]
    pub suite: String,
}

#[derive(Subcommand, Clone)]
pub enum QuarantineSub {
    Add {
        #[arg(long)]
        test_id: String,
        #[arg(long)]
        reason: String,
    },
    Remove {
        #[arg(long)]
        test_id: String,
    },
    List,
}

#[derive(Parser, Clone)]
pub struct TraceArgs {
    #[command(subcommand)]
    pub cmd: TraceSub,
}

#[derive(Subcommand, Clone)]
pub enum TraceSub {
    /// Ingest a raw JSONL log file and normalize to trace dataset
    Ingest {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Ingest OpenTelemetry JSONL traces (GenAI SemConv)
    IngestOtel {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        db: PathBuf,
        /// Optional: Link ingested traces to a new run in this suite
        #[arg(long)]
        suite: Option<String>,

        /// Optional: Write converted trace events to this JSONL file (V2 format) for replay
        #[arg(long)]
        out_trace: Option<PathBuf>,
    },
    /// Verify a trace dataset covers all prompts in eval config
    Verify {
        #[arg(long)]
        trace: PathBuf,
        #[arg(long)]
        config: PathBuf,
    },
    /// Precompute embeddings for trace entries
    PrecomputeEmbeddings {
        #[arg(long)]
        trace: PathBuf,
        #[arg(long)]
        config: PathBuf, // Needed to know which model/embedder to use? Or explicitly pass embedder?
        // Plan says: --trace dataset.jsonl --embedder openai
        // But we also need model info potentially. Let's start with explicit args.
        #[arg(long)]
        embedder: String,
        #[arg(long, default_value = "text-embedding-3-small")]
        model: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Precompute judge scores for trace entries
    PrecomputeJudge {
        #[arg(long)]
        trace: PathBuf,
        #[arg(long)]
        config: PathBuf, // Judge config usually in eval.yaml or separate args?
        // Plan says: --judge openai
        #[arg(long)]
        judge: String,
        #[arg(long)]
        judge_model: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Import an MCP transcript (Inspector/JSON-RPC) and convert to Assay V2 trace
    ImportMcp {
        #[arg(long)]
        input: PathBuf,

        #[arg(long)]
        out_trace: PathBuf,

        /// Input format: inspector | jsonrpc
        #[arg(long, default_value = "inspector")]
        format: String,

        #[arg(long)]
        episode_id: Option<String>,

        #[arg(long)]
        test_id: Option<String>,

        /// User prompt text (strongly recommended for replay strictness)
        #[arg(long)]
        prompt: Option<String>,
    },
}
