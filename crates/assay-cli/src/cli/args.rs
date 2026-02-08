use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "assay",
    version,
    about = "Policy-as-Code for AI Agents — deterministic testing, verifiable evidence, and runtime enforcement for MCP"
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
    /// Watch config/policy/trace files and rerun on changes
    Watch(WatchArgs),
    Import(ImportArgs),
    Migrate(MigrateArgs),
    Coverage(CoverageArgs),
    Explain(super::commands::explain::ExplainArgs),
    Demo(DemoArgs),
    InitCi(InitCiArgs),
    Fix(FixArgs),
    /// Experimental: MCP Process Wrapper
    #[command(hide = true)]
    Mcp(McpArgs),
    Version,
    Policy(PolicyArgs),
    /// Discover MCP servers on this machine (v1.8)
    Discover(DiscoverArgs),
    /// Kill/Terminate MCP servers (v1.8)
    Kill(super::commands::kill::KillArgs),
    /// Runtime eBPF Monitor (Linux only)
    Monitor(super::commands::monitor::MonitorArgs),
    /// Learning Mode: Generate policy from trace or profile
    Generate(super::commands::generate::GenerateArgs),
    /// Learning Mode: Capture and Generate in one flow
    Record(super::commands::record::RecordArgs),
    /// Manage multi-run profiles for stability analysis
    Profile(super::commands::profile::ProfileArgs),
    /// Secure execution sandbox (v0.1)
    Sandbox(SandboxArgs),
    /// Evidence Management (Audit/Compliance)
    Evidence(EvidenceArgs),
    /// Replay bundle management (create/verify)
    Bundle(BundleArgs),
    /// Replay a run from a replay bundle
    Replay(ReplayArgs),
    /// Attack Simulation (Hardening/Compliance)
    #[cfg(feature = "sim")]
    Sim(SimArgs),
    /// Interactive installer and environment setup (Phase 2)
    Setup(SetupArgs),
    /// Tool signing and verification
    Tool(ToolArgs),
}

#[derive(Parser, Debug)]
pub struct ToolArgs {
    #[command(subcommand)]
    pub cmd: super::commands::tool::ToolCmd,
}

#[derive(Parser, Debug)]
pub struct BundleArgs {
    #[command(subcommand)]
    pub cmd: BundleSub,
}

#[derive(Subcommand, Debug)]
pub enum BundleSub {
    /// Create replay bundle from run artifacts
    Create(BundleCreateArgs),
    /// Verify replay bundle integrity and safety
    Verify(BundleVerifyArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct BundleCreateArgs {
    /// Source run path (directory or run.json)
    #[arg(long)]
    pub from: Option<PathBuf>,

    /// Source run id (used for selection and default output naming)
    #[arg(long, conflicts_with = "from")]
    pub run_id: Option<String>,

    /// Output replay bundle path (default: .assay/bundles/<run_id>.tar.gz)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Optional config file to include in bundle
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Optional trace file to include in bundle
    #[arg(long)]
    pub trace_file: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct BundleVerifyArgs {
    /// Replay bundle archive (.tar.gz)
    #[arg(long)]
    pub bundle: PathBuf,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ReplayArgs {
    /// Replay bundle archive (.tar.gz)
    #[arg(long)]
    pub bundle: PathBuf,

    /// Allow live provider calls during replay (default: offline hermetic)
    #[arg(long)]
    pub live: bool,

    /// Override replay seed (order seed)
    #[arg(long)]
    pub seed: Option<u64>,

    /// Exit code compatibility mode
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,
}

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum ValidateOutputFormat {
    #[default]
    Text,
    Json,
    Sarif,
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

    #[arg(long, value_enum, default_value_t = ValidateOutputFormat::Text)]
    pub format: ValidateOutputFormat,

    #[arg(long)]
    pub output: Option<std::path::PathBuf>,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,
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

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,

    /// Exit code compatibility mode: v1 (legacy) or v2 (standard)
    /// defaults to v2 (trace not found = 2)
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,

    /// Disable signature verification (UNSAFE); recorded in summary.json as verify_enabled: false
    #[arg(long)]
    pub no_verify: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            config: PathBuf::from("eval.yaml"),
            db: PathBuf::from(".eval/eval.db"),
            rerun_failures: 0,
            quarantine_mode: "warn".to_string(),
            trace_file: None,
            redact_prompts: false,
            baseline: None,
            export_baseline: None,
            strict: false,
            embedder: "none".to_string(),
            embedding_model: "text-embedding-3-small".to_string(),
            refresh_embeddings: false,
            incremental: false,
            refresh_cache: false,
            no_cache: false,
            explain_skip: false,
            judge: JudgeArgs::default(),
            replay_strict: false,
            deny_deprecations: false,
            exit_codes: crate::exit_codes::ExitCodeVersion::default(),
            no_verify: false,
        }
    }
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

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,

    /// Exit code compatibility mode: v1 (legacy) or v2 (standard)
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,

    /// Disable signature verification (UNSAFE); recorded in summary.json as verify_enabled: false
    #[arg(long)]
    pub no_verify: bool,

    /// Write PR comment body (markdown) to file for GitHub Actions
    #[arg(long)]
    pub pr_comment: Option<PathBuf>,
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

impl Default for JudgeArgs {
    fn default() -> Self {
        Self {
            judge: "none".to_string(),
            no_judge: false,
            judge_model: None,
            judge_samples: 3,
            judge_refresh: false,
            judge_temperature: 0.0,
            judge_max_tokens: 800,
            judge_api_key: None,
        }
    }
}

#[derive(Parser, Clone)]
pub struct InitArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,

    /// generate CI scaffolding (smoke test, traces, workflow)
    /// Pass a provider name (e.g. "github") or leave empty for default.
    #[arg(long, num_args = 0..=1, default_missing_value = "github")]
    pub ci: Option<String>,

    /// generate .gitignore for artifacts/db
    #[arg(long)]
    pub gitignore: bool,

    /// Starter preset to use: default | hardened | dev
    /// Backward-compatible alias: --pack
    #[arg(long = "preset", alias = "pack", default_value = "default")]
    pub preset: String,

    /// List available presets and exit
    /// Backward-compatible alias: --list-packs
    #[arg(long = "list-presets", alias = "list-packs")]
    pub list_presets: bool,

    /// Generate policy from an existing trace file (JSONL events)
    #[arg(long)]
    pub from_trace: Option<PathBuf>,

    /// Enable heuristics (entropy/risk analysis) when generating from trace
    #[arg(long, requires = "from_trace")]
    pub heuristics: bool,

    /// Generate a ready-to-run hello trace + smoke suite scaffold
    #[arg(long, conflicts_with = "from_trace")]
    pub hello_trace: bool,
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
    pub config: Option<std::path::PathBuf>,

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

    /// Diagnose and offer/apply automated fixes for known issues
    #[arg(long)]
    pub fix: bool,

    /// Apply all available fixes without interactive confirmation
    #[arg(long)]
    pub yes: bool,

    /// Preview fixes without writing files
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct WatchArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub trace_file: Option<std::path::PathBuf>,

    #[arg(long)]
    pub baseline: Option<std::path::PathBuf>,

    #[arg(long, default_value = ".eval/eval.db")]
    pub db: std::path::PathBuf,

    #[arg(long)]
    pub strict: bool,

    #[arg(long, default_value = "false")]
    pub replay_strict: bool,

    /// Clear terminal before each rerun
    #[arg(long)]
    pub clear: bool,

    /// Debounce file events before rerunning (milliseconds)
    #[arg(long, default_value_t = 350)]
    pub debounce_ms: u64,
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

#[derive(Parser, Clone, Debug)]
pub struct DemoArgs {
    /// Output directory for demo files
    #[arg(long, default_value = "assay-demo")]
    pub out: PathBuf,
}

#[derive(Parser, Clone, Debug)]
pub struct InitCiArgs {
    /// CI Provider: github | gitlab
    #[arg(long, default_value = "github")]
    pub provider: String,

    /// Output path (default depends on provider)
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Parser, Clone, Debug)]
pub struct McpArgs {
    #[command(subcommand)]
    pub cmd: McpSub,
}

#[derive(Subcommand, Clone, Debug)]
pub enum McpSub {
    /// Wrap an MCP server process
    Wrap(McpWrapArgs),
    /// Detect config path and generate setup
    ConfigPath(ConfigPathArgs),
}

#[derive(Parser, Clone, Debug)]
pub struct ConfigPathArgs {
    /// Client to configure: claude|cursor
    pub client: String,

    /// Policy path to include in config
    #[arg(long)]
    pub policy: Option<String>,

    /// Wrapped server config (command)
    #[arg(long)]
    pub server: Option<String>,

    /// Output JSON only
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct SetupArgs {
    /// Dry run: show what would be installed but do not make changes (default)
    #[arg(long, default_value_t = true)]
    pub dry_run: bool,

    /// Apply changes: actually perform installation (may require sudo)
    #[arg(long, conflicts_with = "dry_run")]
    pub apply: bool,

    /// Install helper binary from this local path (e.g. target/release/assay-bpf)
    #[arg(long)]
    pub helper_from: Option<PathBuf>,

    /// Installation prefix for binary (default: /usr/local/bin)
    #[arg(long, default_value = "/usr/local/bin")]
    pub prefix: PathBuf,

    /// Runtime directory for socket (default: /run/assay)
    #[arg(long, default_value = "/run/assay")]
    pub runtime_dir: PathBuf,

    /// Non-interactive mode (for CI/automation)
    #[arg(long)]
    pub non_interactive: bool,
}

#[derive(Parser, Clone, Debug)]
pub struct McpWrapArgs {
    /// Policy file (default: assay.yaml)
    #[arg(long, default_value = "assay.yaml")]
    pub policy: PathBuf,

    /// Log decisions but do not block
    #[arg(long)]
    pub dry_run: bool,

    /// Print decisions to stderr
    #[arg(long)]
    pub verbose: bool,

    /// Unique label for this server (used for tool identity)
    #[arg(long)]
    pub label: Option<String>,

    /// Write lifecycle events (mandate.used, mandate.revoked) to this NDJSON log.
    /// Requires --event-source. May contain duplicates on retries; deduplicate by CloudEvents.id.
    #[arg(long, requires = "event_source")]
    pub audit_log: Option<PathBuf>,

    /// Write tool decision events (assay.tool.decision) to this NDJSON log.
    /// Requires --event-source.
    #[arg(long, requires = "event_source")]
    pub decision_log: Option<PathBuf>,

    /// CloudEvents source URI (e.g. assay://org/app).
    /// Must be absolute URI with scheme://. Required when --audit-log or --decision-log is set.
    #[arg(long, value_name = "URI")]
    pub event_source: Option<String>,

    /// Command to wrap (use -- to separate args)
    #[arg(required = true, last = true, allow_hyphen_values = true)]
    pub command: Vec<String>,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaxRisk {
    Low,
    Medium,
    High,
}

#[derive(clap::Args, Debug, Clone)]
pub struct FixArgs {
    #[arg(long, default_value = "assay.yaml")]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub trace_file: Option<std::path::PathBuf>,

    #[arg(long)]
    pub baseline: Option<std::path::PathBuf>,

    #[arg(long, default_value = "false")]
    pub replay_strict: bool,

    /// Apply all suggested patches without prompting
    #[arg(long)]
    pub yes: bool,

    /// Do not write files; show diffs of what would change
    #[arg(long)]
    pub dry_run: bool,

    /// Only apply patch(es) with these id(s). Can be repeated.
    #[arg(long)]
    pub only: Vec<String>,

    /// Skip patches above this risk level
    #[arg(long, value_enum, default_value_t = MaxRisk::High)]
    pub max_risk: MaxRisk,

    /// List suggested patches (after --only/--max-risk filtering) and exit
    #[arg(long)]
    pub list: bool,
}

#[derive(clap::Args, Clone, Debug)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub cmd: PolicyCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum PolicyCommand {
    /// Validate policy syntax and (v2) JSON Schemas
    Validate(PolicyValidateArgs),

    /// Migrate v1.x constraints policy to v2.0 schemas
    Migrate(PolicyMigrateArgs),

    /// Format policy YAML (normalizes formatting)
    Fmt(PolicyFmtArgs),
}

#[derive(clap::Args, Clone, Debug)]
pub struct PolicyValidateArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,
}

#[derive(clap::Args, Clone, Debug)]
pub struct PolicyMigrateArgs {
    /// Input policy file (v1.x or v2.0)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (default: overwrite input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Dry run (print to stdout instead of overwriting)
    #[arg(long)]
    pub dry_run: bool,

    /// Preview only (no write)
    #[arg(long)]
    pub check: bool,
}

#[derive(clap::Args, Clone, Debug)]
pub struct EvidenceArgs {
    #[command(subcommand)]
    pub cmd: super::commands::evidence::EvidenceCmd,
}

#[derive(clap::Args, Clone, Debug)]
pub struct PolicyFmtArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (default: overwrite input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DiscoverArgs {
    /// Scan local machine (config files & processes)
    #[clap(long, default_value_t = true)]
    pub local: bool,

    /// Output format (table, json, yaml)
    #[clap(long, default_value = "table")]
    pub format: String,

    /// Write output to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Fail if specific conditions met (comma separated): unmanaged, no_auth
    #[arg(long, value_delimiter = ',')]
    pub fail_on: Option<Vec<String>>,

    /// Policy file to use for configuration
    #[arg(long)]
    pub policy: Option<PathBuf>,
}
#[derive(clap::Args, Debug, Clone)]
pub struct SandboxArgs {
    /// Command to run in the sandbox
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,

    /// Path to policy file (optional)
    #[arg(long, short)]
    pub policy: Option<std::path::PathBuf>,

    /// Working directory for command
    #[arg(long, short)]
    pub workdir: Option<std::path::PathBuf>,

    /// Timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Active enforcement: hard-block unauthorized actions
    #[arg(long)]
    pub enforce: bool,

    /// Dry-run mode: allow and log unauthorized actions (exit 4 if they occur)
    #[arg(long, conflicts_with = "enforce")]
    pub dry_run: bool,

    /// Fail if policy cannot be enforced (exit 2)
    #[arg(long)]
    pub fail_closed: bool,

    /// Strict env mode: only safe base vars + explicit allows
    #[arg(long = "env-strict")]
    pub env_strict: bool,

    /// Strip execution-influence vars (LD_PRELOAD, etc.)
    #[arg(long = "env-strip-exec")]
    pub env_strip_exec: bool,

    /// Allow specific env vars through the filter (comma-separated or repeated)
    #[arg(long = "env-allow", value_delimiter = ',')]
    pub env_allow: Option<Vec<String>>,

    /// DANGER: Pass all env vars without scrubbing
    #[arg(long = "env-passthrough")]
    pub env_passthrough: bool,

    /// Force a safe PATH (/usr/bin:/bin on Linux)
    #[arg(long = "env-safe-path")]
    pub env_safe_path: bool,

    /// Profile execution and generate policy suggestion at this path
    #[arg(long)]
    pub profile: Option<PathBuf>,

    /// Profile output format: yaml | json (default: yaml)
    #[arg(long, default_value = "yaml")]
    pub profile_format: String,

    /// Optional path for human-readable profile report
    #[arg(long)]
    pub profile_report: Option<PathBuf>,

    /// Show detailed sandbox setup
    #[arg(long, short)]
    pub verbose: bool,

    /// Suppress banner output
    #[arg(long, short)]
    pub quiet: bool,
}

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimArgs {
    #[command(subcommand)]
    pub cmd: SimSub,
}

#[cfg(feature = "sim")]
#[derive(Subcommand, Clone, Debug)]
pub enum SimSub {
    /// Run an attack simulation suite
    Run(SimRunArgs),
}

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimRunArgs {
    /// Simulation suite to run (quick, nightly, stress, chaos)
    #[arg(long, default_value = "quick")]
    pub suite: String,

    /// Specific attack vector to run (overrides suite)
    #[arg(long)]
    pub attack: Option<String>,

    /// Target bundle for simulation
    #[arg(long, short)]
    pub target: std::path::PathBuf,

    /// Seed for reproducible mutations
    #[arg(long)]
    pub seed: Option<u64>,

    /// Number of iterations per attack
    #[arg(long)]
    pub iterations: Option<usize>,

    /// Path to write machine-readable JSON report
    #[arg(long)]
    pub report: Option<std::path::PathBuf>,

    /// Directory to write mutated artifacts for forensics
    #[arg(long, short)]
    pub output: Option<std::path::PathBuf>,

    /// Verification limits (preset or JSON)
    #[arg(long)]
    pub limits: Option<String>,
}
