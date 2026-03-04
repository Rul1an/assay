#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;
use tempfile::tempdir;

fn read_json(path: &std::path::Path) -> Value {
    let content = std::fs::read_to_string(path).expect("coverage report should exist");
    serde_json::from_str(&content).expect("coverage report must be valid JSON")
}

#[test]
fn mcp_wrap_coverage_cli_smoke_writes_report() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("normalized-wrap.jsonl");
    let out = dir.path().join("coverage.json");

    std::fs::write(
        &input,
        r#"{"tool":"assay_policy_decide","tool_classes":["sink:network"]}
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(&input)
        .args(["--out"])
        .arg(&out)
        .args(["--declared-tool", "assay_policy_decide"])
        .assert()
        .success();

    let report = read_json(&out);
    assert_eq!(report["schema_version"], "coverage_report_v1");
    assert_eq!(report["run"]["source"], "jsonl");
    assert_eq!(
        report["tools"]["tools_seen"],
        serde_json::json!(["assay_policy_decide"])
    );
    assert_eq!(
        report["taxonomy"]["tool_classes_seen"],
        serde_json::json!(["sink:network"])
    );
}
