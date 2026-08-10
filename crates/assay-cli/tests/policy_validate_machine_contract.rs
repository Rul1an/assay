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

    std::fs::write(dir.path().join("malformed.yaml"), "version: [\n")
        .expect("write malformed policy");
    let malformed_args = [
        "policy",
        "validate",
        "--input",
        "malformed.yaml",
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
            .is_some_and(|message| message.contains("malformed.yaml")),
        "failure must identify the policy path"
    );
    assert!(
        malformed_json["next_step"]
            .as_str()
            .is_some_and(|next| next.starts_with("Run: assay ") && next.contains("malformed.yaml")),
        "an agent must receive a concrete recovery command"
    );

    let stderr = String::from_utf8_lossy(&malformed.stderr);
    assert!(stderr.contains("E_POLICY_PARSE"));

    let recovery = malformed_json["next_step"]
        .as_str()
        .expect("failure next_step")
        .strip_prefix("Run: assay ")
        .expect("next_step must invoke assay");
    let recovery_args = recovery.split_whitespace().collect::<Vec<_>>();
    let recovered = assay(dir.path(), &recovery_args);
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
