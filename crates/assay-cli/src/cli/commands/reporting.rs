use super::pipeline::PipelineTimings;
use super::run_output::{export_baseline, summary_from_outcome, write_run_json_minimal};
use crate::exit_codes::{ExitCodeVersion, ReasonCode, RunOutcome};
use std::path::{Path, PathBuf};

pub(crate) fn write_error_artifacts(
    reason: ReasonCode,
    message: String,
    context: Option<&str>,
    version: ExitCodeVersion,
    verify_enabled: bool,
    run_json_path: &Path,
) -> anyhow::Result<i32> {
    let mut o = RunOutcome::from_reason(reason, Some(message), context);
    o.exit_code = reason.exit_code_for(version);
    if let Err(e) = write_run_json_minimal(&o, run_json_path) {
        eprintln!("WARNING: failed to write run.json: {}", e);
    }

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let summary = summary_from_outcome(&o, verify_enabled).with_seeds(None, None);
    if let Err(e) = assay_core::report::summary::write_summary(&summary, &summary_path) {
        eprintln!("WARNING: failed to write summary.json: {}", e);
    }
    Ok(o.exit_code)
}
