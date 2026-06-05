use std::path::PathBuf;

use assay_runner_core::RunSpec;
use assay_runner_schema::SDK_EVENT_SCHEMA;

use super::args::RunnerSpikeRunArgs;

pub(super) fn validate_runner_spike_args(args: &RunnerSpikeRunArgs) -> anyhow::Result<()> {
    if args.sdk_event_log.is_some() && args.agent_shim == "none" {
        anyhow::bail!("runner-spike --sdk-event-log requires an SDK agent shim");
    }
    Ok(())
}

pub(super) fn build_spec(args: &RunnerSpikeRunArgs) -> RunSpec {
    let mut spec = RunSpec::new(args.command.clone()).with_agent_shim(args.agent_shim.clone());
    if let Some(run_id) = &args.run_id {
        spec = spec.with_run_id(run_id.clone());
    }
    if let Some(path) = &args.sdk_event_log {
        let run_id = spec.run_id.clone();
        spec = spec
            .with_env("ASSAY_RUNNER_SDK_EVENT_LOG", path.display().to_string())
            .with_env("ASSAY_RUNNER_RUN_ID", run_id)
            .with_env("ASSAY_RUNNER_SDK_EVENT_SCHEMA", SDK_EVENT_SCHEMA);
    }
    spec
}

pub(super) fn bundle_output_path(args: &RunnerSpikeRunArgs, run_id: &str) -> PathBuf {
    args.output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("assay-runner-spike-{run_id}.tar.gz")))
}
