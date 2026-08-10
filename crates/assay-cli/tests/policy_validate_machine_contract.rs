//! Binary contract for policy validation's human and machine output modes (#2162).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const LIMITS: ProcessLimits = ProcessLimits::new(Duration::from_secs(5), 64 * 1024, 64 * 1024);
const SUMMARY_SCHEMA: &str = "assay.run_summary.v1";

fn assay<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Output {
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
    command.current_dir(cwd).env("NO_COLOR", "1").args(args);
    run_bounded(command, b"", LIMITS, "policy validation contract")
        .unwrap_or_else(|error| panic!("{error}"))
}

fn initialized_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let init = assay(dir.path(), &["init", "--preset", "dev", "--hello-trace"]);
    assert_eq!(
        init.status.code(),
        Some(0),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    dir
}

fn parse_json(output: &Output, expected_exit: i32, context: &str) -> Value {
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

fn recovery_argv(summary: &Value) -> Vec<String> {
    let encoded = summary["next_step"]
        .as_str()
        .expect("failure next_step")
        .strip_prefix("Run argv: ")
        .expect("next_step must publish JSON argv");
    serde_json::from_str(encoded).expect("next_step argv must be valid JSON")
}

fn assert_unclassified_failure(output: &Output, context: &str) {
    assert_eq!(output.status.code(), Some(2), "{context} must fail");
    assert!(
        output.stdout.is_empty(),
        "{context} has no honest typed JSON contract yet"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fatal:"),
        "{context} must retain the legacy untyped failure path: {stderr}"
    );
    assert!(
        !stderr.contains("E_POLICY_PARSE"),
        "{context} must not be misclassified as a YAML parse failure: {stderr}"
    );
}

#[test]
fn default_mode_keeps_the_existing_human_channels() {
    let dir = initialized_project();
    let valid = assay(
        dir.path(),
        &["policy", "validate", "--input", "policy.yaml"],
    );
    assert_eq!(valid.status.code(), Some(0));
    assert!(
        valid.stdout.is_empty(),
        "default success stdout must stay empty"
    );
    assert!(
        String::from_utf8_lossy(&valid.stderr).contains("Policy OK"),
        "default success must remain human-readable on stderr"
    );

    std::fs::write(dir.path().join("malformed.yaml"), "version: [\n")
        .expect("write malformed policy");
    let malformed = assay(
        dir.path(),
        &["policy", "validate", "--input", "malformed.yaml"],
    );
    assert_eq!(malformed.status.code(), Some(2));
    assert!(
        malformed.stdout.is_empty(),
        "default failure stdout must stay empty"
    );
    assert!(
        !malformed.stderr.is_empty(),
        "default failure must remain human-readable on stderr"
    );
}

#[test]
fn json_mode_reports_valid_and_malformed_policies_on_stdout() {
    let dir = initialized_project();
    let valid_args = [
        "policy",
        "validate",
        "--input",
        "policy.yaml",
        "--format",
        "json",
    ];
    let valid = assay(dir.path(), &valid_args);
    let valid_json = parse_json(&valid, 0, "valid policy");
    assert_eq!(valid_json["schema"], SUMMARY_SCHEMA);
    assert_eq!(valid_json["schema_version"], 1);
    assert_eq!(valid_json["exit_code"], 0);
    assert_eq!(valid_json["reason_code"], "");
    assert!(valid_json.get("next_step").is_none());
    assert!(
        valid_json.get("message").is_none(),
        "policy validation must not claim that tests ran"
    );

    // This also pins the serde_yaml error downcast: if the loader backend or
    // wrapping changes, E_POLICY_PARSE must not silently disappear.
    let malformed_name = "pol icy;$(echo x).yaml";
    std::fs::write(dir.path().join(malformed_name), "version: [\n")
        .expect("write malformed policy");
    let malformed_args = [
        "policy",
        "validate",
        "--input",
        malformed_name,
        "--format",
        "json",
    ];
    let malformed = assay(dir.path(), &malformed_args);
    let malformed_json = parse_json(&malformed, 2, "malformed policy");
    assert_eq!(malformed_json["schema"], SUMMARY_SCHEMA);
    assert_eq!(malformed_json["schema_version"], 1);
    assert_eq!(malformed_json["exit_code"], 2);
    assert_eq!(malformed_json["reason_code"], "E_POLICY_PARSE");
    assert!(
        malformed_json["message"]
            .as_str()
            .is_some_and(|message| message.contains(malformed_name)),
        "failure must identify the policy path"
    );

    let stderr = String::from_utf8_lossy(&malformed.stderr);
    assert!(stderr.contains("E_POLICY_PARSE"));

    let recovery = recovery_argv(&malformed_json);
    assert_eq!(
        recovery,
        ["assay", "policy", "validate", "--input", malformed_name],
        "the hostile path must remain one argv element"
    );
    let recovered = assay(dir.path(), &recovery[1..]);
    assert_eq!(recovered.status.code(), Some(2));
    let recovered_stderr = String::from_utf8_lossy(&recovered.stderr);
    assert!(
        recovered_stderr.contains("E_POLICY_PARSE"),
        "the published next_step must reach validation, not Clap usage: {recovered_stderr}"
    );
    assert!(!recovered_stderr.contains("Usage:"));

    let valid_again = assay(dir.path(), &valid_args);
    let malformed_again = assay(dir.path(), &malformed_args);
    assert_eq!(
        valid.stdout, valid_again.stdout,
        "valid JSON must be stable"
    );
    assert_eq!(
        malformed.stdout, malformed_again.stdout,
        "failure JSON must be stable"
    );
}

#[test]
fn json_mode_does_not_misclassify_untyped_policy_failures() {
    let dir = initialized_project();
    let missing = assay(
        dir.path(),
        &[
            "policy",
            "validate",
            "--input",
            "missing.yaml",
            "--format",
            "json",
        ],
    );
    assert_unclassified_failure(&missing, "missing policy");

    std::fs::write(
        dir.path().join("invalid-schema.yaml"),
        r#"version: "2.0"
tools:
  allow: [demo]
schemas:
  demo:
    type: object
    properties:
      value:
        type: string
        pattern: "["
"#,
    )
    .expect("write invalid schema policy");
    let invalid_schema = assay(
        dir.path(),
        &[
            "policy",
            "validate",
            "--input",
            "invalid-schema.yaml",
            "--format",
            "json",
        ],
    );
    assert_unclassified_failure(&invalid_schema, "invalid policy schema");
}
