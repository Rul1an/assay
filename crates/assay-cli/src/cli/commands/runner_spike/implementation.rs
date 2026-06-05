use std::fs::File;

use super::args::{RunnerSpikeArgs, RunnerSpikeCommand, RunnerSpikeRunArgs};
use super::exit_status::{exit_status_code, exit_status_label};
use super::logs::apply_policy_then_sdk_logs_if_requested;
use super::spec::{build_spec, bundle_output_path, validate_runner_spike_args};

pub async fn run(args: RunnerSpikeArgs) -> anyhow::Result<i32> {
    match args.cmd {
        RunnerSpikeCommand::Run(args) => cmd_run(args).await,
    }
}

async fn cmd_run(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    validate_runner_spike_args(&args)?;
    if args.kernel_capture {
        return super::cgroup::cmd_run_with_kernel_capture(args).await;
    }

    cmd_run_contract_only(args)
}

fn cmd_run_contract_only(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    let spec = build_spec(&args);
    let output = bundle_output_path(&args, &spec.run_id);

    let mut outcome = spec.run_contract_only()?;
    apply_policy_then_sdk_logs_if_requested(&spec, &args, &mut outcome.archive)?;
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
