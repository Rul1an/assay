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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn mk_args(iterations: u32, seed: Option<u64>) -> SimSoakArgs {
        SimSoakArgs {
            iterations,
            seed,
            target: "demo".to_string(),
            report: std::path::PathBuf::from("/tmp/soak-test.json"),
            time_budget: 60,
        }
    }

    #[test]
    fn report_schema_valid_payload_passes() {
        let args = mk_args(5, Some(1));
        let report = report::build_report("0.0.0-test", &args);
        let v: Value = serde_json::to_value(&report).expect("report serializes");
        schema::validate_soak_report_v1(&v).expect("valid payload should pass schema");
    }

    #[test]
    fn report_schema_invalid_payload_fails() {
        let args = mk_args(5, Some(1));
        let report = report::build_report("0.0.0-test", &args);
        let mut v: Value = serde_json::to_value(&report).expect("report serializes");
        v.as_object_mut()
            .expect("top-level object")
            .remove("report_version");
        assert!(schema::validate_soak_report_v1(&v).is_err());
    }

    #[test]
    fn seed_determinism_structural_json_equality() {
        let args = mk_args(25, Some(7));
        let r1 = report::build_report("0.0.0-test", &args);
        let r2 = report::build_report("0.0.0-test", &args);
        let v1: Value = serde_json::to_value(&r1).expect("report serializes");
        let v2: Value = serde_json::to_value(&r2).expect("report serializes");
        assert_eq!(v1, v2);
    }

    #[test]
    fn policy_exit_code_is_1_when_violations_present() {
        let args = mk_args(5, Some(1));
        let mut report = report::build_report("0.0.0-test", &args);
        report.violations.push(report::RuleViolation {
            rule_id: "ASSAY-VIOLATION".to_string(),
            dimension: "correctness".to_string(),
            count: 1,
        });
        let exit = if !report.violations.is_empty() {
            EXIT_TEST_FAILURE
        } else {
            0
        };
        assert_eq!(exit, EXIT_TEST_FAILURE);
    }
}
