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
        // Non-zero because an error-severity diagnostic survives the repair, not a specific class.
        // Which class that is belongs to `doctor_exit_class_contract.rs`, which pins it as the one
        // `decide_exit` gives for the diagnostic; asserting the number here too would make this a
        // second answer to that question and the two would be free to drift.
        .assert()
        .failure();

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
        // See above: this test owns "dry run writes nothing", not the exit class.
        .assert()
        .failure();

    assert!(
        !trace.exists(),
        "doctor --fix --dry-run --yes should not create trace file"
    );
}

#[test]
fn doctor_fix_parse_error_dry_run_exits_nonzero_and_does_not_write() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");

    fs::write(
        &config,
        "version: 1\nsuite: doctor-fix\nmodel: trace\nsettngs: {}\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write invalid eval config");
    let before = fs::read_to_string(&config).expect("read before");

    let mut cmd = Command::cargo_bin("assay").expect("cargo bin");
    let assert = cmd
        .current_dir(temp.path())
        .arg("doctor")
        .arg("--config")
        .arg(&config)
        .arg("--fix")
        .arg("--dry-run")
        .arg("--yes")
        .assert()
        .failure();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = config.display();
    let misspelled_line = before.lines().nth(3).expect("fixture has misspelled key");
    assert!(
        stdout.contains(&format!("--- {path} (dry-run) patch=rename_config_key ---")),
        "dry-run output must identify the file and patch; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("--- {path}\n+++ {path}\n")),
        "dry-run output must contain unified-diff file headers; stdout:\n{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line.starts_with("@@ ")),
        "dry-run output must contain a unified-diff hunk; stdout:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == format!("-{misspelled_line}"))
            && stdout.lines().any(|line| line == "+settings: {}"),
        "dry-run output must show the old key removed and replacement added; stdout:\n{stdout}"
    );
    assert!(
        !stdout
            .lines()
            .any(|line| line == format!("+{misspelled_line}"))
            && !stdout.lines().any(|line| line == "-settings: {}"),
        "dry-run output must preserve diff direction; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("--- end ---\nDry run complete. 1 fix(es) previewed."),
        "dry-run output must close the preview before its summary; stdout:\n{stdout}"
    );

    let after = fs::read_to_string(&config).expect("read after");
    assert_eq!(
        before, after,
        "doctor --fix --dry-run must not modify config file"
    );
}

#[test]
fn doctor_yes_without_fix_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");

    write_minimal_config(&config);

    let mut cmd = Command::cargo_bin("assay").expect("cargo bin");
    cmd.current_dir(temp.path())
        .arg("doctor")
        .arg("--config")
        .arg(&config)
        .arg("--trace-file")
        .arg(&trace)
        .arg("--yes")
        .assert()
        .code(2);

    assert!(
        !trace.exists(),
        "doctor --yes without --fix must not write trace file"
    );
}
