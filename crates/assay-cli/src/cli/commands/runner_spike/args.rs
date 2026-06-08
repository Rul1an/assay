use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Redaction mode for evidence captured by this run (ADR-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "snake_case")]
pub enum RedactArg {
    /// Curated shape rules plus flag-aware argv value redaction (default).
    #[default]
    ShapeAndFlag,
    /// Curated shape rules only.
    ShapeOnly,
}

/// Where the redaction key (placeholder salt) comes from (ADR-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "kebab-case")]
pub enum RedactionKeyArg {
    /// Runner-local key file (ASSAY_REDACTION_KEY_FILE env, else the default host path, generated
    /// once). Tokens correlate across runs on the same host (default).
    #[default]
    HostLocal,
    /// In-memory throwaway key, never persisted. Tokens do not correlate with any other run.
    Ephemeral,
}

#[derive(Debug, Clone, Args)]
pub struct RunnerSpikeArgs {
    #[command(subcommand)]
    pub cmd: RunnerSpikeCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RunnerSpikeCommand {
    /// Run a command under the Phase 1 runner-spike contract boundary.
    Run(RunnerSpikeRunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RunnerSpikeRunArgs {
    /// Agent runtime shim to declare for this run.
    #[arg(long, default_value = "none")]
    pub agent_shim: String,

    /// Explicit run id. Defaults to a generated stream-safe id.
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output bundle path. Defaults to assay-runner-spike-<run_id>.tar.gz.
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Hidden S3 spike path: capture live kernel events with assay-monitor.
    #[arg(long, hide = true)]
    pub kernel_capture: bool,

    /// eBPF object path for hidden kernel capture mode.
    #[arg(long, hide = true)]
    pub ebpf: Option<PathBuf>,

    /// Milliseconds to drain kernel events after the child exits.
    #[arg(long, hide = true, default_value_t = 100)]
    pub kernel_drain_ms: u64,

    /// Hidden S4 spike path: ingest assay mcp wrap --decision-log output.
    #[arg(long, hide = true)]
    pub policy_decision_log: Option<PathBuf>,

    /// Hidden S5 spike path: ingest normalized SDK event NDJSON.
    #[arg(long, hide = true)]
    pub sdk_event_log: Option<PathBuf>,

    /// Hidden overhead experiment path: write runner phase timing diagnostics.
    #[arg(long, hide = true)]
    pub phase_timing_log: Option<PathBuf>,

    /// Evidence redaction mode (ADR-034). Secret-shaped values in argv and the capability surface are
    /// replaced with a value-free placeholder before the bundle is hashed.
    #[arg(long, value_enum, default_value_t = RedactArg::ShapeAndFlag)]
    pub redact: RedactArg,

    /// Where the redaction key comes from. host-local persists a per-host key; ephemeral is throwaway.
    #[arg(long, value_enum, default_value_t = RedactionKeyArg::HostLocal)]
    pub redaction_key: RedactionKeyArg,

    /// DANGER: write raw, un-redacted evidence. The bundle may contain credentials and must not be
    /// shared or retained. Recorded in observation_health as disabled_unsafe.
    #[arg(long)]
    pub unsafe_disable_redaction: bool,

    /// Command to run.
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}
