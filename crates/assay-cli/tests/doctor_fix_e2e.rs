#![allow(deprecated)]

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn write_minimal_config(path: &std::path::Path) {
    fs::write(
        path,
        "version: 1\nsuite: doctor-fix\nmodel: trace\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write eval config");
}

#[test]
fn doctor_fix_yes_creates_missing_trace_file() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");

    write_minimal_config(&config);
    assert!(!trace.exists());

    let mut cmd = Command::cargo_bin("assay").expect("cargo bin");
    cmd.current_dir(temp.path())
        .arg("doctor")
        .arg("--config")
        .arg(&config)
        .arg("--trace-file")
        .arg(&trace)
        .arg("--fix")
        .arg("--yes")
        .assert()
        .code(1);

    assert!(
        trace.exists(),
        "doctor --fix --yes should create trace file"
    );
    let content = fs::read_to_string(&trace).expect("read trace");
    assert!(content.is_empty(), "created trace should be empty");
}

#[test]
fn doctor_fix_dry_run_does_not_write_trace_file() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");

    write_minimal_config(&config);
    assert!(!trace.exists());

    let mut cmd = Command::cargo_bin("assay").expect("cargo bin");
    cmd.current_dir(temp.path())
        .arg("doctor")
        .arg("--config")
        .arg(&config)
        .arg("--trace-file")
        .arg(&trace)
        .arg("--fix")
        .arg("--dry-run")
        .arg("--yes")
        .assert()
        .success();

    assert!(
        !trace.exists(),
        "doctor --fix --dry-run --yes should not create trace file"
    );
}
