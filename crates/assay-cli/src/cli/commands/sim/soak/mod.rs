use std::fs;

use crate::cli::args::SimSoakArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_TEST_FAILURE};
use anyhow::{Context, Result};

mod report;
mod schema;

pub fn run(args: SimSoakArgs) -> Result<i32> {
    if args.time_budget == 0 {
        eprintln!("Config error: --time-budget must be > 0");
        return Ok(EXIT_CONFIG_ERROR);
    }
    if args.iterations == 0 {
        eprintln!("Config error: --iterations must be > 0");
        return Ok(EXIT_CONFIG_ERROR);
    }

    let report = report::build_report(env!("CARGO_PKG_VERSION"), &args);
    let report_value =
        serde_json::to_value(&report).context("failed to serialize soak report to JSON value")?;

    if let Err(e) = schema::validate_soak_report_v1(&report_value) {
        eprintln!("{e}");
        return Ok(EXIT_CONFIG_ERROR);
    }

    let report_json =
        serde_json::to_string_pretty(&report_value).context("failed to encode soak report JSON")?;
    if let Err(e) = fs::write(&args.report, report_json) {
        eprintln!(
            "Infra error: failed to write report {}: {e}",
            args.report.display()
        );
        return Ok(EXIT_INFRA_ERROR);
    }

    if report::measurement_exceeded(&report) {
        eprintln!(
            "Config error: soak measurement exceeded time budget (duration={}s > budget={}s)",
            report.run.duration_seconds, report.run.time_budget_seconds
        );
        return Ok(EXIT_CONFIG_ERROR);
    }

    if !report.violations.is_empty() {
        return Ok(EXIT_TEST_FAILURE);
    }

    Ok(0)
}
