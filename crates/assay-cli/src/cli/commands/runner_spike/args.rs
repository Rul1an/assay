use clap::{Args, Subcommand};
use std::path::PathBuf;

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

    /// Command to run.
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}
