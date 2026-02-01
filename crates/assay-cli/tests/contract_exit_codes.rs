#![allow(deprecated)]
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

fn read_run_json(dir: &std::path::Path) -> Value {
    let path = dir.join("run.json");
    if !path.exists() {
        panic!("run.json missing in {}", dir.display());
    }
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&content).expect("Invalid JSON in run.json")
}

fn assert_schema(v: &Value) {
    assert!(
        v.get("exit_code").expect("missing exit_code").is_i64(),
        "exit_code must be int"
    );
    assert!(
        v.get("reason_code")
            .expect("missing reason_code")
            .is_string(),
        "reason_code must be string"
    );
    if let Some(w) = v.get("warnings") {
        let arr = w.as_array().expect("warnings must be array");
        for item in arr {
            assert!(item.is_string(), "warning items must be strings");
        }
    }
}
// ...
#[test]
fn contract_ci_report_io_failure() {
    let dir = tempdir().unwrap();
    // Valid config, 1 passing test
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .unwrap();

    // Output is a directory -> IO Error
    let bad_path = dir.path().join("bad_output");
    fs::create_dir(&bad_path).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("ci")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--junit")
        .arg(&bad_path)
        .arg("--sarif")
        .arg(&bad_path)
        .assert()
        .success(); // Option B: Success

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 0);

    // Verification of Machine-Readable Warnings (Strict)
    let warnings = v
        .get("warnings")
        .expect("warnings field missing in run.json")
        .as_array()
        .expect("warnings must be an array");

    // Expect exactly 2 warnings (JUnit and SARIF)
    assert_eq!(
        warnings.len(),
        2,
        "Expected exactly 2 warnings (JUnit + SARIF)"
    );

    let has_junit = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write JUnit"));
    let has_sarif = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write SARIF"));

    assert!(
        has_junit,
        "Missing JUnit warning in run.json. Found: {:?}",
        warnings
    );
    assert!(
        has_sarif,
        "Missing SARIF warning in run.json. Found: {:?}",
        warnings
    );
}

#[test]
fn contract_run_json_always_written_arg_conflict() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--baseline")
        .arg("dummy")
        .arg("--export-baseline")
        .arg("dummy")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_INVALID_ARGS");
}

#[test]
fn contract_reason_code_trace_not_found_v2() {
    let dir = tempdir().unwrap();
    // Valid config schema with ID
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
}

#[test]
fn contract_legacy_v1_trace_not_found() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v1")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(3);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 3);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
}

#[test]
fn contract_exit_codes_missing_config() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("non_existent.yaml")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
}
