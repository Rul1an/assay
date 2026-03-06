use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

    /// Generate a coverage_report_v1 from wrap decision events at session end.
    /// Requires --event-source. If --decision-log is omitted, a temporary decision log is used.
    #[arg(long, requires = "event_source")]
    pub coverage_out: Option<PathBuf>,

    /// Write a session_state_window_v1 informational report after the wrapped MCP session completes.
    #[arg(long, requires = "event_source")]
    pub state_window_out: Option<PathBuf>,

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
