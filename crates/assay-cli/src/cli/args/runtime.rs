use clap::Parser;
use std::path::PathBuf;

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
