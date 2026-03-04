#![allow(deprecated)]

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use std::path::PathBuf;
use tempfile::tempdir;

fn read_json(path: &std::path::Path) -> Value {
    let content = std::fs::read_to_string(path).expect("coverage report should exist");
    serde_json::from_str(&content).expect("coverage report must be valid JSON")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/coverage")
        .join(name)
}

#[test]
fn coverage_contract_generates_valid_report_from_basic_jsonl() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.json");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_basic.jsonl"))
        .args(["--out"])
        .arg(&out)
        .args([
            "--declared-tool",
            "read_document",
            "--declared-tool",
            "web_search",
            "--declared-tool",
            "web_search_alt",
        ])
        .assert()
        .success();

    let report = read_json(&out);
    assert_eq!(report["schema_version"], "coverage_report_v1");
    assert_eq!(
        report["tools"]["tools_unknown"],
        serde_json::json!(["unknown_tool_x"])
    );
    assert_eq!(
        report["taxonomy"]["tool_classes_missing"],
        serde_json::json!(["unknown_tool_x"])
    );
    assert_eq!(
        report["routes"]["routes_seen"],
        serde_json::json!([
            {"count": 1, "from": "read_document", "to": "web_search_alt"},
            {"count": 1, "from": "web_search", "to": "unknown_tool_x"},
            {"count": 1, "from": "web_search_alt", "to": "web_search"}
        ])
    );
}

#[test]
fn coverage_contract_accepts_tool_name_fallback_jsonl() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.json");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_tool_name_fallback.jsonl"))
        .args(["--out"])
        .arg(&out)
        .args([
            "--declared-tool",
            "read_document",
            "--declared-tool",
            "web_search",
            "--declared-tool",
            "web_search_alt",
        ])
        .assert()
        .success();

    let report = read_json(&out);
    assert_eq!(report["schema_version"], "coverage_report_v1");
    assert_eq!(
        report["tools"]["tools_seen"],
        serde_json::json!(["read_document", "web_search", "web_search_alt"])
    );
}

#[test]
fn coverage_contract_fails_when_tool_fields_missing() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.json");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_missing_tool_fields.jsonl"))
        .args(["--out"])
        .arg(&out)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("missing required field: 'tool' or 'tool_name'"));
}
