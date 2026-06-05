use assay_runner_core::RunSpec;

use super::args::RunnerSpikeRunArgs;

pub(super) fn apply_policy_then_sdk_logs_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
) -> anyhow::Result<()> {
    // Policy must be applied before SDK: SDK cross-checks read policy
    // correlation bindings and the mismatch determinism gate relies on stable
    // ambiguity ordering.
    apply_policy_decision_log_if_requested(spec, args, archive)?;
    apply_sdk_event_log_if_requested(spec, args, archive)?;
    Ok(())
}

fn apply_policy_decision_log_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
) -> anyhow::Result<()> {
    let Some(path) = args.policy_decision_log.as_ref() else {
        return Ok(());
    };

    let bytes = std::fs::read(path).map_err(|error| {
        anyhow::anyhow!(
            "failed to read runner-spike policy decision log {}: {error}",
            path.display()
        )
    })?;
    let capture =
        assay_runner_core::PolicyLayerCapture::from_decision_ndjson(spec.run_id.clone(), &bytes)?;
    capture.apply_to_archive(archive)?;
    Ok(())
}

fn apply_sdk_event_log_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
) -> anyhow::Result<()> {
    let Some(path) = args.sdk_event_log.as_ref() else {
        return Ok(());
    };

    let bytes = std::fs::read(path).map_err(|error| {
        anyhow::anyhow!(
            "failed to read runner-spike SDK event log {}: {error}",
            path.display()
        )
    })?;
    let capture = assay_runner_core::SdkLayerCapture::from_sdk_ndjson(spec.run_id.clone(), &bytes)?;
    capture.apply_to_archive(archive)?;
    Ok(())
}
