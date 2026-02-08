#![allow(deprecated)]
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn read_json(path: &Path) -> Value {
    let content = fs::read_to_string(path).expect("missing json file");
    serde_json::from_str(&content).expect("invalid json")
}

fn read_run_json(dir: &Path) -> Value {
    read_json(&dir.join("run.json"))
}

fn read_summary_json(dir: &Path) -> Value {
    read_json(&dir.join("summary.json"))
}

fn run_assay(dir: &Path, subcmd: &str, args: &[&str], expected_code: i32) {
    let mut cmd = Command::cargo_bin("assay").expect("cargo bin");
    cmd.current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .arg(subcmd);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.assert().code(expected_code);
}

fn assert_failure_contract(run: &Value, summary: &Value) {
    assert!(
        run.get("exit_code").and_then(Value::as_i64).is_some(),
        "run.json must include integer exit_code"
    );
    assert!(
        run.get("reason_code").and_then(Value::as_str).is_some(),
        "run.json must include reason_code"
    );
    assert_eq!(
        run.get("exit_code"),
        summary.get("exit_code"),
        "run.json and summary.json must agree on exit code"
    );
    assert_eq!(
        run.get("reason_code"),
        summary.get("reason_code"),
        "run.json and summary.json must agree on reason code"
    );
}

#[test]
fn parity_missing_config_run_vs_ci() {
    let run_dir = tempdir().expect("tempdir");
    let ci_dir = tempdir().expect("tempdir");

    run_assay(run_dir.path(), "run", &["--config", "missing.yaml"], 2);
    run_assay(ci_dir.path(), "ci", &["--config", "missing.yaml"], 2);

    let run_run = read_run_json(run_dir.path());
    let run_summary = read_summary_json(run_dir.path());
    let ci_run = read_run_json(ci_dir.path());
    let ci_summary = read_summary_json(ci_dir.path());

    assert_failure_contract(&run_run, &run_summary);
    assert_failure_contract(&ci_run, &ci_summary);
    assert_eq!(run_run["reason_code"], "E_MISSING_CONFIG");
    assert_eq!(ci_run["reason_code"], "E_MISSING_CONFIG");
}

#[test]
fn parity_invalid_args_baseline_export_run_vs_ci() {
    let run_dir = tempdir().expect("tempdir");
    let ci_dir = tempdir().expect("tempdir");

    run_assay(
        run_dir.path(),
        "run",
        &["--baseline", "a.json", "--export-baseline", "b.json"],
        2,
    );
    run_assay(
        ci_dir.path(),
        "ci",
        &["--baseline", "a.json", "--export-baseline", "b.json"],
        2,
    );

    let run_run = read_run_json(run_dir.path());
    let run_summary = read_summary_json(run_dir.path());
    let ci_run = read_run_json(ci_dir.path());
    let ci_summary = read_summary_json(ci_dir.path());

    assert_failure_contract(&run_run, &run_summary);
    assert_failure_contract(&ci_run, &ci_summary);
    assert_eq!(run_run["reason_code"], "E_INVALID_ARGS");
    assert_eq!(ci_run["reason_code"], "E_INVALID_ARGS");
}

#[test]
fn parity_deny_deprecations_run_vs_ci() {
    let run_dir = tempdir().expect("tempdir");
    let ci_dir = tempdir().expect("tempdir");
    let eval = r#"configVersion: 1
suite: strict-deprecations
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
      type: args_valid
      policy: policy.yaml
"#;
    fs::write(run_dir.path().join("eval.yaml"), eval).expect("write eval");
    fs::write(ci_dir.path().join("eval.yaml"), eval).expect("write eval");

    run_assay(
        run_dir.path(),
        "run",
        &["--config", "eval.yaml", "--deny-deprecations"],
        2,
    );
    run_assay(
        ci_dir.path(),
        "ci",
        &["--config", "eval.yaml", "--deny-deprecations"],
        2,
    );

    let run_run = read_run_json(run_dir.path());
    let run_summary = read_summary_json(run_dir.path());
    let ci_run = read_run_json(ci_dir.path());
    let ci_summary = read_summary_json(ci_dir.path());

    assert_failure_contract(&run_run, &run_summary);
    assert_failure_contract(&ci_run, &ci_summary);
    assert_eq!(run_run["reason_code"], "E_CFG_PARSE");
    assert_eq!(ci_run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn ci_report_outputs_contract_default_names_and_non_blocking_failures() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .expect("write config");

    run_assay(dir.path(), "ci", &["--config", "assay.yaml"], 0);
    assert!(
        dir.path().join("junit.xml").exists(),
        "ci must write default junit.xml"
    );
    assert!(
        dir.path().join("sarif.json").exists(),
        "ci must write default sarif.json"
    );

    let bad_path = dir.path().join("bad_output");
    fs::create_dir(&bad_path).expect("create bad output");
    run_assay(
        dir.path(),
        "ci",
        &[
            "--config",
            "assay.yaml",
            "--junit",
            "bad_output",
            "--sarif",
            "bad_output",
        ],
        0,
    );

    let run = read_run_json(dir.path());
    let warnings = run
        .get("warnings")
        .and_then(Value::as_array)
        .expect("warnings must be present for report write failures");
    assert!(
        warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|w| w.contains("Failed to write JUnit")),
        "run.json warnings must include JUnit write failure"
    );
    assert!(
        warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|w| w.contains("Failed to write SARIF")),
        "run.json warnings must include SARIF write failure"
    );
}
