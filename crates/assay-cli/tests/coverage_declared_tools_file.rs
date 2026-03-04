#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;
use tempfile::tempdir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/coverage")
        .join(name)
}

#[test]
fn coverage_declared_tools_file_union_with_flags() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.json");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_basic.jsonl"))
        .args(["--out"])
        .arg(&out)
        .args(["--declared-tools-file"])
        .arg(fixture_path("declared_tools_basic.txt"))
        .args(["--declared-tool", "web_search_alt"])
        .assert()
        .success();

    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(&out).expect("coverage json should be written"),
    )
    .expect("coverage report must be valid JSON");

    assert_eq!(
        report["tools"]["tools_declared"],
        serde_json::json!(["read_document", "web_search", "web_search_alt"])
    );
    assert_eq!(
        report["tools"]["tools_unknown"],
        serde_json::json!(["unknown_tool_x"])
    );
}
