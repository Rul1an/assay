use super::super::super::args::ReplayArgs;
use super::provenance::annotate_run_json_provenance;
use crate::exit_codes::{ReasonCode, RunOutcome};
use std::path::PathBuf;

pub(super) fn write_missing_dependency(
    args: &ReplayArgs,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
    message: String,
) -> anyhow::Result<i32> {
    write_replay_failure(
        args,
        bundle_digest,
        replay_mode,
        source_run_id,
        ReasonCode::EReplayMissingDependency,
        message,
        Some("assay replay --bundle <path> --live"),
    )
}

pub(super) fn write_replay_failure(
    args: &ReplayArgs,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
    reason: ReasonCode,
    message: String,
    next_step_override: Option<&str>,
) -> anyhow::Result<i32> {
    let mut outcome = RunOutcome::from_reason(reason, Some(message), None);
    if let Some(next_step) = next_step_override {
        outcome.next_step = Some(next_step.to_string());
    }
    outcome.exit_code = reason.exit_code_for(args.exit_codes);

    let run_json_path = PathBuf::from("run.json");
    if let Err(err) = super::super::run_output::write_run_json_minimal(&outcome, &run_json_path) {
        eprintln!("warning: failed to write run.json: {}", err);
    }
    if let Err(err) = annotate_run_json_provenance(
        &run_json_path,
        bundle_digest,
        replay_mode,
        source_run_id.as_deref(),
    ) {
        eprintln!("warning: failed to annotate run.json provenance: {}", err);
    }

    let summary_path = PathBuf::from("summary.json");
    // Explicit early-exit seed policy: null seeds because replay run did not execute.
    let summary = super::super::run_output::summary_from_outcome(&outcome, true)
        .with_seeds(None, None)
        .with_replay_provenance(bundle_digest.to_string(), replay_mode, source_run_id);
    if let Err(err) = assay_core::report::summary::write_summary(&summary, &summary_path) {
        eprintln!("warning: failed to write summary.json: {}", err);
    }

    Ok(outcome.exit_code)
}
