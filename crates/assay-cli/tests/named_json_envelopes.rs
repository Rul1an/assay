//! Named identities for the JSON documents a non-interactive validate/run caller receives.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const VALIDATE_REPORT_SCHEMA: &str = "assay.validate_report.v1";
const RUN_REPORT_SCHEMA: &str = "assay.run_report.v1";
const RUN_SUMMARY_SCHEMA: &str = "assay.run_summary.v1";

fn assay(cwd: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run assay binary")
}

fn stdout_json(output: &Output, expected_exit: i32, context: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "{context} exit differed; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context} stdout is not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_schema(document: &Value, expected: &str, context: &str) {
    assert_eq!(document["schema"], expected, "{context} schema identity");
    assert_eq!(
        document["schema_version"], 1,
        "{context} schema version must be an integer scoped by schema"
    );
}

#[test]
fn validate_and_run_json_documents_name_the_contract_they_follow() {
    let project = tempfile::tempdir().expect("project tempdir");
    let init = assay(
        project.path(),
        &["init", "--preset", "dev", "--hello-trace"],
    );
    assert_eq!(
        init.status.code(),
        Some(0),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let validate_success = stdout_json(
        &assay(
            project.path(),
            &[
                "validate",
                "--config",
                "eval.yaml",
                "--trace-file",
                "traces/hello.jsonl",
                "--format",
                "json",
            ],
        ),
        0,
        "validate success",
    );
    assert_schema(
        &validate_success,
        VALIDATE_REPORT_SCHEMA,
        "validate success",
    );

    let validate_failure = stdout_json(
        &assay(
            project.path(),
            &["validate", "--config", "missing.yaml", "--format", "json"],
        ),
        2,
        "validate failure",
    );
    assert_schema(
        &validate_failure,
        VALIDATE_REPORT_SCHEMA,
        "validate failure",
    );

    let run_success = stdout_json(
        &assay(
            project.path(),
            &[
                "run",
                "--config",
                "eval.yaml",
                "--trace-file",
                "traces/hello.jsonl",
                "--format",
                "json",
            ],
        ),
        0,
        "run success",
    );
    assert_schema(&run_success, RUN_REPORT_SCHEMA, "run success");

    let summary: Value = serde_json::from_slice(
        &std::fs::read(project.path().join("summary.json")).expect("read summary.json"),
    )
    .expect("summary.json parses");
    assert_schema(&summary, RUN_SUMMARY_SCHEMA, "summary.json");

    let failure_project = tempfile::tempdir().expect("failure project tempdir");
    let run_failure = stdout_json(
        &assay(
            failure_project.path(),
            &["run", "--config", "missing.yaml", "--format", "json"],
        ),
        2,
        "run failure",
    );
    assert_schema(&run_failure, RUN_SUMMARY_SCHEMA, "run failure");

    let failure_summary: Value = serde_json::from_slice(
        &std::fs::read(failure_project.path().join("summary.json"))
            .expect("read failure summary.json"),
    )
    .expect("failure summary.json parses");
    assert_schema(&failure_summary, RUN_SUMMARY_SCHEMA, "failure summary.json");

    let identities = [
        validate_success["schema"].as_str(),
        run_success["schema"].as_str(),
        run_failure["schema"].as_str(),
    ];
    assert_eq!(
        identities,
        [
            Some(VALIDATE_REPORT_SCHEMA),
            Some(RUN_REPORT_SCHEMA),
            Some(RUN_SUMMARY_SCHEMA),
        ],
        "the three documents must remain distinguishable"
    );
}
