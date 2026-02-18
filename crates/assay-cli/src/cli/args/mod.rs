use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub mod baseline;
pub mod bundle;
pub mod common;
pub mod policy;
pub mod run;
pub use baseline::*;
pub use bundle::*;
pub use common::*;
pub use policy::*;
pub use run::*;

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
pub struct EvidenceArgs {
    #[command(subcommand)]
    pub cmd: super::commands::evidence::EvidenceCmd,
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
    /// Run soak reliability simulation
    Soak(SimSoakArgs),
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

    /// Target bundle for simulation (not required with --print-config)
    #[arg(long, short, required_unless_present = "print_config")]
    pub target: Option<std::path::PathBuf>,

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

    /// Verification limits as JSON, or @path to load from file
    #[arg(long)]
    pub limits: Option<String>,

    /// Path to JSON file with limits (overrides --limits if both given)
    #[arg(long)]
    pub limits_file: Option<std::path::PathBuf>,

    /// Suite time budget in seconds (default: 60). Must be > 0.
    #[arg(long, default_value = "60")]
    pub time_budget: u64,

    /// Print effective limits and time budget, then exit
    #[arg(long)]
    pub print_config: bool,
}

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimSoakArgs {
    /// Number of iterations to run (default: 20). Must be > 0.
    #[arg(long, default_value = "20")]
    pub iterations: u32,

    /// RNG seed for deterministic runs (optional).
    #[arg(long)]
    pub seed: Option<u64>,

    /// Target identifier (e.g. bundle/scenario id)
    #[arg(long)]
    pub target: String,

    /// Output path for soak report JSON
    #[arg(long)]
    pub report: std::path::PathBuf,

    /// Suite time budget in seconds (default: 60). Must be > 0.
    #[arg(long, default_value = "60")]
    pub time_budget: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::Parser;

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }

    #[cfg(feature = "sim")]
    #[test]
    fn sim_soak_parses_with_defaults() {
        let cli = Cli::try_parse_from([
            "assay", "sim", "soak", "--target", "bundle", "--report", "out.json",
        ])
        .expect("parse should succeed");

        match cli.cmd {
            Command::Sim(sim) => match sim.cmd {
                SimSub::Soak(args) => {
                    assert_eq!(args.iterations, 20);
                    assert_eq!(args.time_budget, 60);
                    assert_eq!(args.seed, None);
                    assert_eq!(args.target, "bundle");
                }
                _ => panic!("expected SimSub::Soak"),
            },
            _ => panic!("expected Command::Sim"),
        }
    }

    #[cfg(feature = "sim")]
    #[test]
    fn sim_soak_parses_explicit_values() {
        let cli = Cli::try_parse_from([
            "assay",
            "sim",
            "soak",
            "--iterations",
            "5",
            "--seed",
            "42",
            "--target",
            "scenario-a",
            "--report",
            "out.json",
            "--time-budget",
            "120",
        ])
        .expect("parse should succeed");

        match cli.cmd {
            Command::Sim(sim) => match sim.cmd {
                SimSub::Soak(args) => {
                    assert_eq!(args.iterations, 5);
                    assert_eq!(args.seed, Some(42));
                    assert_eq!(args.target, "scenario-a");
                    assert_eq!(args.time_budget, 120);
                }
                _ => panic!("expected SimSub::Soak"),
            },
            _ => panic!("expected Command::Sim"),
        }
    }
}
