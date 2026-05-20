use assay_runner_spike::RunSpec;
use clap::{Args, Subcommand};
use std::fs::File;
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

    /// Command to run.
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

pub async fn run(args: RunnerSpikeArgs) -> anyhow::Result<i32> {
    match args.cmd {
        RunnerSpikeCommand::Run(args) => cmd_run(args),
    }
}

fn cmd_run(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    let mut spec = RunSpec::new(args.command).with_agent_shim(args.agent_shim);
    if let Some(run_id) = args.run_id {
        spec = spec.with_run_id(run_id);
    }

    let output = args
        .output
        .unwrap_or_else(|| PathBuf::from(format!("assay-runner-spike-{}.tar.gz", spec.run_id)));

    let outcome = spec.run_contract_only()?;
    let mut file = File::create(&output)?;
    outcome.archive.write(&mut file)?;
    let exit_status = exit_status_label(outcome.exit_code, outcome.signal);

    println!(
        "wrote runner-spike bundle: {} (run_id={}, status={})",
        output.display(),
        spec.run_id,
        exit_status
    );

    Ok(exit_status_code(outcome.exit_code, outcome.signal))
}

fn exit_status_label(exit_code: Option<i32>, signal: Option<i32>) -> String {
    match (exit_code, signal) {
        (Some(code), _) => format!("exit_code:{code}"),
        (None, Some(signal)) => format!("signal:{signal}"),
        (None, None) => "unknown".to_string(),
    }
}

fn exit_status_code(exit_code: Option<i32>, signal: Option<i32>) -> i32 {
    match (exit_code, signal) {
        (Some(code), _) => code,
        (None, Some(signal)) => 128 + signal,
        (None, None) => 1,
    }
}
