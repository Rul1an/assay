use assay_runner_core::RunSpec;

use super::args::RunnerSpikeRunArgs;

pub(super) fn apply_policy_then_sdk_logs_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
) -> anyhow::Result<()> {
    apply_policy_then_sdk_logs_inner(spec, args, archive, None)
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn apply_policy_then_sdk_logs_with_session_cgroup_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
    session_cgroup_id: u64,
) -> anyhow::Result<()> {
    apply_policy_then_sdk_logs_inner(spec, args, archive, Some(session_cgroup_id))
}

fn apply_policy_then_sdk_logs_inner(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
    session_cgroup_id: Option<u64>,
) -> anyhow::Result<()> {
    // Policy must be applied before SDK: SDK cross-checks read policy
    // correlation bindings and the mismatch determinism gate relies on stable
    // ambiguity ordering.
    apply_policy_decision_log_if_requested(spec, args, archive, session_cgroup_id)?;
    apply_sdk_event_log_if_requested(spec, args, archive)?;
    Ok(())
}

fn apply_policy_decision_log_if_requested(
    spec: &RunSpec,
    args: &RunnerSpikeRunArgs,
    archive: &mut assay_runner_core::RunnerSpikeArchive,
    session_cgroup_id: Option<u64>,
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
    if let Some(session_cgroup_id) = session_cgroup_id {
        capture.apply_to_archive_with_session_cgroup(archive, session_cgroup_id)?;
    } else {
        capture.apply_to_archive(archive)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::runner_spike::args::{RedactArg, RedactionKeyArg};
    use assay_runner_schema::{CgroupCorrelationStatus, KernelLayerStatus};

    fn args_with_policy_decision_log(path: std::path::PathBuf) -> RunnerSpikeRunArgs {
        RunnerSpikeRunArgs {
            agent_shim: "none".to_string(),
            run_id: Some("run_001".to_string()),
            output: None,
            kernel_capture: false,
            ebpf: None,
            kernel_drain_ms: 100,
            policy_decision_log: Some(path),
            sdk_event_log: None,
            phase_timing_log: None,
            redact: RedactArg::ShapeAndFlag,
            redaction_key: RedactionKeyArg::Ephemeral,
            unsafe_disable_redaction: false,
            command: vec!["true".to_string()],
        }
    }

    #[test]
    fn session_cgroup_id_flows_into_policy_correlation_binding() {
        let dir = tempfile::tempdir().unwrap();
        let policy_log = dir.path().join("policy.ndjson");
        std::fs::write(
            &policy_log,
            br#"{"type":"assay.tool.decision","source":"assay://runner-spike/run_001","data":{"tool":"read_file","decision":"allow","tool_call_id":"tc_runner_policy_001"}}
"#,
        )
        .unwrap();
        let args = args_with_policy_decision_log(policy_log);
        let spec = RunSpec::new(args.command.clone()).with_run_id("run_001");
        let mut archive = assay_runner_core::RunnerSpikeArchive::empty("run_001", "linux");
        archive.observation_health.kernel_layer = KernelLayerStatus::Complete;
        archive.observation_health.cgroup_correlation = CgroupCorrelationStatus::Clean;
        archive.kernel_layer_ndjson = b"{\"schema\":\"assay.runner.kernel_event.v0\"}\n".to_vec();

        apply_policy_then_sdk_logs_with_session_cgroup_if_requested(&spec, &args, &mut archive, 42)
            .unwrap();

        assert_eq!(archive.correlation_report.bindings.len(), 1);
        assert_eq!(archive.correlation_report.bindings[0].cgroup_id, Some(42));
    }
}
