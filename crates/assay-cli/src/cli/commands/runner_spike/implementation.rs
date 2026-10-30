use std::fs::File;

use super::args::{RunnerSpikeArgs, RunnerSpikeCommand, RunnerSpikeRunArgs, RunnerSpikeVerifyArgs};
use super::exit_status::{exit_status_code, exit_status_label};
use super::logs::apply_policy_then_sdk_logs_if_requested;
use super::spec::{build_spec, bundle_output_path, validate_runner_spike_args};

pub async fn run(args: RunnerSpikeArgs) -> anyhow::Result<i32> {
    match args.cmd {
        RunnerSpikeCommand::Run(args) => cmd_run(args).await,
        RunnerSpikeCommand::Verify(args) => cmd_verify(args),
    }
}

fn cmd_verify(args: RunnerSpikeVerifyArgs) -> anyhow::Result<i32> {
    assay_evidence::runner_archive::verify_runner_archive(
        File::open(&args.archive)?,
        &args.sha256,
        &args.run_id,
        assay_evidence::VerifyLimits::for_retained_events(),
    )?;
    let report = serde_json::json!({
        "archive_integrity": "verified",
        "run_id": args.run_id,
        "sha256": args.sha256,
        "layer_semantics": "not_verified",
        "coverage": "not_verified",
        "authenticity": "not_established"
    });
    Ok(crate::output_write::write_stdout_json(
        &serde_json::to_string(&report)?,
    ))
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
    // Redact secret-shaped values and run the fail-closed sweep before writing (and hashing).
    super::redaction::finalize_redaction(&args, &mut outcome.archive)?;
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
