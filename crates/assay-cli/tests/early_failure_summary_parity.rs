//! Parity test pinning early-failure stdout against summary.json bytes (issue #2171).
//!
//! A failure before test results exist emits an `assay.run_summary.v1` document to disk
//! at `summary.json` and, under `--format json`, to stdout. The two outputs must be produced
//! by the same renderer, byte-for-byte identical except for stdout's single trailing newline.

use std::path::Path;
use std::process::{Command, Output};

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

#[test]
fn early_failure_run_stdout_matches_summary_json_bytes_exactly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing_config = dir.path().join("nonexistent_missing_config.yaml");

    let out = assay(
        dir.path(),
        &[
            "run",
            "--config",
            missing_config.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit code 2 for early failure; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let summary_path = dir.path().join("summary.json");
    let summary_bytes = std::fs::read(&summary_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", summary_path.display()));
    assert!(!summary_bytes.is_empty(), "summary.json must not be empty");

    assert!(
        !out.stdout.is_empty(),
        "stdout must not be empty for --format json"
    );
    assert!(
        out.stdout.ends_with(b"\n"),
        "stdout must end with exactly a trailing newline"
    );

    let stdout_without_trailing_newline = &out.stdout[..out.stdout.len() - 1];

    assert_eq!(
        stdout_without_trailing_newline,
        summary_bytes.as_slice(),
        "stdout bytes (excluding trailing newline) must equal summary.json bytes byte-for-byte"
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&summary_bytes).expect("summary.json must be valid JSON");
    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 2);
    assert_eq!(parsed["reason_code"], "E_MISSING_CONFIG");
}

#[test]
fn early_failure_ci_stdout_matches_summary_json_bytes_exactly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing_config = dir.path().join("nonexistent_missing_config.yaml");

    let out = assay(
        dir.path(),
        &[
            "ci",
            "--config",
            missing_config.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit code 2 for early failure; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let summary_path = dir.path().join("summary.json");
    let summary_bytes = std::fs::read(&summary_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", summary_path.display()));
    assert!(!summary_bytes.is_empty(), "summary.json must not be empty");

    assert!(
        out.stdout.ends_with(b"\n"),
        "stdout must end with exactly a trailing newline"
    );

    let stdout_without_trailing_newline = &out.stdout[..out.stdout.len() - 1];

    assert_eq!(
        stdout_without_trailing_newline,
        summary_bytes.as_slice(),
        "stdout bytes (excluding trailing newline) must equal summary.json bytes byte-for-byte"
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&summary_bytes).expect("summary.json must be valid JSON");
    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 2);
    assert_eq!(parsed["reason_code"], "E_MISSING_CONFIG");
}
