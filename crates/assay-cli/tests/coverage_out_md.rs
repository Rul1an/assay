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
fn coverage_out_md_writes_json_and_markdown_artifacts() {
    let dir = tempdir().unwrap();
    let out_json = dir.path().join("coverage.json");
    let out_md = dir.path().join("coverage.md");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_basic.jsonl"))
        .args(["--out"])
        .arg(&out_json)
        .args(["--out-md"])
        .arg(&out_md)
        .args([
            "--declared-tool",
            "read_document",
            "--declared-tool",
            "web_search",
            "--declared-tool",
            "web_search_alt",
            "--format",
            "md",
        ])
        .assert()
        .success();

    let json_text = std::fs::read_to_string(&out_json).expect("json report should exist");
    let report: Value = serde_json::from_str(&json_text).expect("json report must parse");
    assert_eq!(report["schema_version"], "coverage_report_v1");

    let markdown = std::fs::read_to_string(&out_md).expect("markdown report should exist");
    assert!(markdown.contains("# Coverage Report"));
    assert!(markdown.contains("## Top Routes"));
}
