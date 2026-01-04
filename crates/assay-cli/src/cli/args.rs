use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "assay",
    version,
    about = "CI-first PR regression gate for RAG pipelines (skeleton)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Run(RunArgs),
    Ci(CiArgs),
    Init(InitArgs),
    Quarantine(QuarantineArgs),
    Trace(TraceArgs),
    Calibrate(CalibrateArgs),
    Baseline(BaselineArgs),
    Validate(ValidateArgs),
    Doctor(DoctorArgs),
    Import(ImportArgs),
    Migrate(MigrateArgs),
    Coverage(CoverageArgs),
    Explain(super::commands::explain::ExplainArgs),
    Version,
}

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

    #[arg(long, default_value = "text")]
    pub format: String, // text|json
}

#[derive(Parser, Clone)]
pub struct BaselineArgs {
    #[command(subcommand)]
    pub cmd: BaselineSub,
}

#[derive(Subcommand, Clone)]
pub enum BaselineSub {
    //     /// Generate a hygiene report for a suite
    Report(BaselineReportArgs),
    /// Record the latest run as a baseline
    Record(BaselineRecordArgs),
    /// Check the latest run against a baseline
    Check(BaselineCheckArgs),
}

#[derive(Parser, Clone)]
pub struct BaselineRecordArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name (if omitted, inferred from config)
    #[arg(long)]
    pub suite: Option<String>,

    /// Run ID to record (default: latest for suite)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output path
    #[arg(long, default_value = "assay-baseline.json")]
    pub out: PathBuf,
}

#[derive(Parser, Clone)]
pub struct BaselineCheckArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name (if omitted, inferred from config)
    #[arg(long)]
    pub suite: Option<String>,

    /// Run ID to check (default: latest for suite)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Baseline path
    #[arg(long, default_value = "assay-baseline.json")]
    pub baseline: PathBuf,

    /// Fail on regression
    #[arg(long, default_value_t = true)]
    pub fail_on_regression: bool,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Parser, Clone)]
pub struct BaselineReportArgs {
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name
    #[arg(long)]
    pub suite: String,

    /// Number of recent runs to include
    #[arg(long, default_value_t = 50)]
    pub last: u32,

    /// Output path (JSON or Markdown based on extension or format)
    #[arg(long, default_value = "hygiene.json")]
    pub out: PathBuf,

    /// Output format: json | md
    #[arg(long, default_value = "json")]
    pub format: String,
}

#[derive(Parser, Clone)]
pub struct RunArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    #[arg(long, default_value_t = 0)]
    pub rerun_failures: u32,

    /// quarantine mode: off|warn|strict (controls status of quarantined tests)
    #[arg(long, default_value = "warn")]
    pub quarantine_mode: String,

    /// Trace file to use as Source of Truth for replay (auto-ingested in strict mode)
    #[arg(long)]
    pub trace_file: Option<PathBuf>,

    #[arg(long)]
    pub redact_prompts: bool,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    /// strict mode (controls exit code policy: warn/flaky -> exit 1)
    #[arg(long)]
    pub strict: bool,

    /// embedder provider (none|openai|fake)
    #[arg(long, default_value = "none")]
    pub embedder: String,

    /// embedding model name
    #[arg(long, default_value = "text-embedding-3-small")]
    pub embedding_model: String,

    /// force refresh of embeddings (ignore cache)
    #[arg(long)]
    pub refresh_embeddings: bool,

    /// enable incremental execution (skip passing tests with same fingerprint)
    #[arg(long)]
    pub incremental: bool,

    /// ignore incremental cache (force re-run)
    #[arg(long)]
    pub refresh_cache: bool,

    /// Explicitly disable cache usage (alias for --refresh-cache)
    #[arg(long)]
    pub no_cache: bool,

    /// show details for skipped tests
    #[arg(long)]
    pub explain_skip: bool,

    #[command(flatten)]
    pub judge: JudgeArgs,

    /// strict replay mode: use trace-file as truth, forbid network, auto-ingest to DB
    #[arg(long)]
    pub replay_strict: bool,
}

#[derive(Parser, Clone)]
pub struct CiArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,
    #[arg(long, default_value = "junit.xml")]
    pub junit: PathBuf,
    #[arg(long, default_value = "sarif.json")]
    pub sarif: PathBuf,

    #[arg(long, default_value_t = 2)]
    pub rerun_failures: u32,
    #[arg(long, default_value = "warn")]
    pub quarantine_mode: String,

    #[arg(long)]
    pub otel_jsonl: Option<PathBuf>,

    /// Trace file to use as Source of Truth for replay (auto-ingested in strict mode)
    #[arg(long)]
    pub trace_file: Option<PathBuf>,

    #[arg(long)]
    pub redact_prompts: bool,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    /// strict mode (controls exit code policy: warn/flaky -> exit 1)
    #[arg(long)]
    pub strict: bool,

    #[arg(long, default_value = "none")]
    pub embedder: String,

    #[arg(long, default_value = "text-embedding-3-small")]
    pub embedding_model: String,

    #[arg(long)]
    pub refresh_embeddings: bool,

    /// enable incremental execution (skip passing tests with same fingerprint)
    #[arg(long)]
    pub incremental: bool,

    /// ignore incremental cache (force re-run)
    #[arg(long)]
    pub refresh_cache: bool,

    /// Explicitly disable cache usage (alias for --refresh-cache)
    #[arg(long)]
    pub no_cache: bool,

    /// show details for skipped tests
    #[arg(long)]
    pub explain_skip: bool,

    #[command(flatten)]
    pub judge: JudgeArgs,

    /// strict replay mode: use trace-file as truth, forbid network, auto-ingest to DB
    #[arg(long)]
    pub replay_strict: bool,
}

#[derive(clap::Args, Clone)]
pub struct JudgeArgs {
    /// Enable or disable LLM-as-judge evaluation
    /// - none: judge calls disabled (replay/trace-only)
    /// - openai: live judge calls via OpenAI
    /// - fake: deterministic fake judge (tests/dev)
    #[arg(long, default_value = "none", env = "VERDICT_JUDGE")]
    pub judge: String,

    /// Alias for --judge none
    #[arg(long, conflicts_with = "judge")]
    pub no_judge: bool,

    /// Judge model identifier (provider-specific)
    /// Example: gpt-4o-mini
    #[arg(long, env = "VERDICT_JUDGE_MODEL")]
    pub judge_model: Option<String>,

    /// Number of judge samples per test (majority vote)
    /// Default: 3
    /// Tip: for critical production gates consider: --judge-samples 5
    #[arg(long, default_value_t = 3, env = "VERDICT_JUDGE_SAMPLES")]
    pub judge_samples: u32,

    /// Ignore judge cache and re-run judge calls (live mode only)
    #[arg(long)]
    pub judge_refresh: bool,

    /// Temperature used for judge calls (affects cache key)
    /// Default: 0.0
    #[arg(long, default_value_t = 0.0, env = "VERDICT_JUDGE_TEMPERATURE")]
    pub judge_temperature: f32,

    /// Max tokens for judge response (affects cache key)
    /// Default: 800
    #[arg(long, default_value_t = 800, env = "VERDICT_JUDGE_MAX_TOKENS")]
    pub judge_max_tokens: u32,

    /// Start with env (VERDICT_JUDGE_API_KEY could be supported but OPENAI_API_KEY is primary)
    #[arg(long, hide = true)]
    pub judge_api_key: Option<String>,
}

#[derive(Parser, Clone)]
pub struct InitArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,

    /// generate CI scaffolding (smoke test, traces, workflow)
    #[arg(long)]
    pub ci: bool,

    /// generate .gitignore for artifacts/db
    #[arg(long)]
    pub gitignore: bool,
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
#[derive(Parser, Clone)]
pub struct CalibrateArgs {
    /// Path to a run.json file to analyze (if omitted, reads from DB)
    #[arg(long)]
    pub run: Option<PathBuf>,

    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name (required if using --db)
    #[arg(long)]
    pub suite: Option<String>,

    /// Number of recent runs to include from DB
    #[arg(long, default_value_t = 200)]
    pub last: u32,

    /// Output JSON path
    #[arg(long, default_value = "calibration.json")]
    pub out: PathBuf,

    /// Target tail for recommended min score (e.g. 0.10 for p10)
    #[arg(long, default_value_t = 0.10)]
    pub target_tail: f64,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DoctorArgs {
    #[arg(long)]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub trace_file: Option<std::path::PathBuf>,

    #[arg(long)]
    pub baseline: Option<std::path::PathBuf>,

    #[arg(long)]
    pub db: Option<std::path::PathBuf>,

    #[arg(long, default_value = "false")]
    pub replay_strict: bool,

    #[arg(long, default_value = "text")]
    pub format: String, // text|json

    #[arg(long)]
    pub out: Option<std::path::PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ImportArgs {
    /// Input file (MCP transcript or Inspector JSON)
    pub input: std::path::PathBuf,

    /// Input format: inspector | jsonrpc
    #[arg(long, default_value = "inspector")]
    pub format: String,

    /// Generate initial eval config and policy
    #[arg(long)]
    pub init: bool,

    /// Output trace file path (default: derived from input name)
    #[arg(long)]
    pub out_trace: Option<std::path::PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct MigrateArgs {
    #[arg(long, default_value = "mcp-eval.yaml")]
    pub config: std::path::PathBuf,

    /// Dry run (print to stdout instead of overwriting)
    #[arg(long)]
    pub dry_run: bool,

    /// Check if migration is needed (exit 2 if needed, 0 if clean)
    #[arg(long)]
    pub check: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct CoverageArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub policy: Option<PathBuf>,

    #[arg(long, alias = "traces")]
    pub trace_file: std::path::PathBuf,

    #[arg(long, default_value_t = 0.0)]
    pub min_coverage: f64,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    #[arg(long, default_value = "text")]
    pub format: String, // text|json|markdown|github
}
