#[cfg(target_os = "linux")]
pub(super) fn record_phase(
    phases: &mut std::collections::BTreeMap<&'static str, f64>,
    name: &'static str,
    start: std::time::Instant,
) {
    phases.insert(name, start.elapsed().as_secs_f64() * 1000.0);
}

#[cfg(target_os = "linux")]
pub(super) fn write_phase_timing_log(
    path: Option<&std::path::PathBuf>,
    spec: &assay_runner_core::RunSpec,
    phases: &std::collections::BTreeMap<&'static str, f64>,
    exit_code: Option<i32>,
    signal: Option<i32>,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::json!({
        "schema": "assay.experiment.runner_phase_timing.v0",
        "run_id": &spec.run_id,
        "agent_shim": &spec.agent_shim,
        "phases_ms": phases,
        "exit_code": exit_code,
        "signal": signal,
        "error": error,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use assay_runner_core::RunSpec;

    use super::*;

    #[test]
    fn phase_timing_log_is_experiment_scoped_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("phase-timing.json");
        let spec = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_agent_shim("openai-agents");
        let mut phases = std::collections::BTreeMap::new();
        phases.insert("child_runtime_ms", 12.5);

        write_phase_timing_log(Some(&path), &spec, &phases, Some(0), None, None).unwrap();

        let payload: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(payload["schema"], "assay.experiment.runner_phase_timing.v0");
        assert_eq!(payload["run_id"], "run_001");
        assert_eq!(payload["phases_ms"]["child_runtime_ms"], 12.5);
        assert_eq!(payload["exit_code"], 0);
        assert!(payload["error"].is_null());
    }
}
