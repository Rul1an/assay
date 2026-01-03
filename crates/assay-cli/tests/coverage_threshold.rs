use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_coverage_threshold_success() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("eval.yaml");
    let policy_path = temp.path().join("policy.yaml");
    let traces_path = temp.path().join("traces.jsonl");

    // Policy with 1 tool
    fs::write(
        &policy_path,
        r#"
type: policy
version: "1.1"
name: test_policy
tools:
  allow: [ToolA]
"#,
    )
    .unwrap();

    // Config referencing policy
    fs::write(
        &config_path,
        r#"
version: 1
suite: test_suite
model: fake
tests:
  - id: test1
    input: "foo"
    expected:
       type: args_valid
       policy: "policy.yaml"
"#,
    )
    .unwrap();

    // Trace covering the tool
    fs::write(
        &traces_path,
        r#"
{"trace_id": "1", "tools": ["ToolA"], "rules_triggered": []}
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
        .arg("--config")
        .arg(&config_path)
        .arg("--trace-file")
        .arg(&traces_path)
        .arg("--min-coverage")
        .arg("100") // Should pass
        .assert()
        .success();
}

#[test]
fn test_coverage_threshold_failure() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("eval.yaml");
    let policy_path = temp.path().join("policy.yaml");
    let traces_path = temp.path().join("traces.jsonl");

    // Policy with 2 tools
    fs::write(
        &policy_path,
        r#"
type: policy
version: "1.1"
name: test_policy
tools:
  allow: [ToolA, ToolB]
"#,
    )
    .unwrap();

    // Config referencing policy
    fs::write(
        &config_path,
        r#"
version: 1
suite: test_suite
model: fake
tests:
  - id: test1
    input: "foo"
    expected:
       type: args_valid
       policy: "policy.yaml"
"#,
    )
    .unwrap();

    // Trace covering only 1 tool -> 50% coverage
    fs::write(
        &traces_path,
        r#"
{"trace_id": "1", "tools": ["ToolA"], "rules_triggered": []}
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
        .arg("--config")
        .arg(&config_path)
        .arg("--trace-file")
        .arg(&traces_path)
        .arg("--min-coverage")
        .arg("80") // Should fail (50 < 80)
        .assert()
        .failure()
        .code(1) // TEST_FAILED code is 1
        .stderr(predicate::str::contains("Coverage threshold not met"));
}
