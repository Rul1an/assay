use super::super::args::RunArgs;
use super::pipeline::{
    build_summary_from_artifacts, execute_pipeline, maybe_export_baseline, print_pipeline_summary,
    write_error_artifacts, PipelineError, PipelineInput,
};
use super::run_output::reason_code_from_run_error;
use super::run_output::write_extended_run_json;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) async fn run(args: RunArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    let input = PipelineInput::from_run(&args);
    let execution = execute_pipeline(&input, legacy_mode).await;
    let execution = match execution {
        Ok(ok) => ok,
        Err(PipelineError::Classified { run_error }) => {
            let reason = reason_code_from_run_error(&run_error)
                .unwrap_or(crate::exit_codes::ReasonCode::ECfgParse);
            return write_error_artifacts(
                reason,
                run_error.message,
                version,
                !args.no_verify,
                &run_json_path,
            );
        }
        Err(PipelineError::Fatal(err)) => return Err(err),
    };

    let cfg = execution.cfg;
    let artifacts = execution.artifacts;
    let outcome = execution.outcome;
    let timings = execution.timings;
    let report_start = Instant::now();
    // Use extended writer for authoritative reason coding in run.json (no SARIF in run command)
    write_extended_run_json(&artifacts, &outcome, &run_json_path, None)?;

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let mut summary =
        build_summary_from_artifacts(&outcome, !args.no_verify, &artifacts, Some(&timings), None);

    print_pipeline_summary(&artifacts, args.explain_skip, &summary);

    maybe_export_baseline(&args.export_baseline, &args.config, &cfg, &artifacts);

    // Measure the full reporting phase (outputs + summary prep + console + baseline export).
    let report_ms = report_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    summary = build_summary_from_artifacts(
        &outcome,
        !args.no_verify,
        &artifacts,
        Some(&timings),
        Some(report_ms),
    );
    assay_core::report::summary::write_summary(&summary, &summary_path)?;

    Ok(outcome.exit_code)
}
