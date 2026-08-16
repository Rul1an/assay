use super::super::args::RunArgs;
use super::pipeline::{execute_pipeline, PipelineInput};
use super::pipeline_error::elapsed_ms;
use super::reporting::{
    build_summary_from_artifacts, maybe_export_baseline, print_pipeline_summary,
};
use super::run_output::write_extended_run_json;
use crate::exit_codes::EXIT_SUCCESS;
use crate::output_write::write_stdout_json;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) async fn run(args: RunArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    let input = PipelineInput::from_run(&args);
    let execution = match execute_pipeline(&input, legacy_mode).await {
        Ok(ok) => ok,
        Err(e) => {
            return e.into_exit_code(
                version,
                !args.no_verify,
                &run_json_path,
                matches!(args.format, super::super::args::OutputFormat::Json),
            )
        }
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

    // Output model:
    // - text: human-readable summary on stderr (default; unchanged behavior).
    // - json: machine-readable report on stdout, so `assay run --format json
    //   > results.json` composes with CI pipelines.
    // run.json is written before this display step. A JSON stdout write
    // failure returns exit 3 without writing summary.json or exporting a
    // baseline.
    match args.format {
        super::super::args::OutputFormat::Text => {
            print_pipeline_summary(&artifacts, args.explain_skip, &summary);
        }
        super::super::args::OutputFormat::Json => {
            let write_code = write_stdout_json(&assay_core::report::json::render_json(&artifacts)?);
            if write_code != EXIT_SUCCESS {
                return Ok(write_code);
            }
        }
    }

    maybe_export_baseline(&args.export_baseline, &args.config, &cfg, &artifacts);

    // Measure the full reporting phase (outputs + summary prep + console + baseline export).
    let report_ms = elapsed_ms(report_start);
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
