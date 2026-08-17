#![allow(deprecated)]

use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::tempdir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/coverage")
        .join(name)
}

#[test]
fn coverage_format_md_generates_summary() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.md");

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
            "--declared-tool",
            "*",
            "--format",
            "md",
        ])
        .assert()
        .success();

    let markdown = std::fs::read_to_string(&out).expect("markdown output should be written");
    assert!(markdown.contains("# Coverage Report"));
    assert!(markdown.contains("| tools_declared | 4 |"));
    assert!(markdown.contains("tools_unknown"));
    assert!(markdown.contains("`unknown_tool_x`"));
    assert!(markdown.contains("| `read_document` | `web_search_alt` | 1 |"));
    assert!(markdown.contains(
        "Caller-supplied declarations are literal values; wildcard-looking values are not expanded or filtered."
    ));
}

#[test]
fn coverage_input_preserves_star_declaration_literally() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("coverage.json");

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(fixture_path("input_basic.jsonl"))
        .args(["--out"])
        .arg(&out)
        .args(["--declared-tool", "*", "--format", "json"])
        .assert()
        .success();

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).expect("JSON output should be written"))
            .expect("coverage output should be valid JSON");
    assert_eq!(report["tools"]["tools_declared"], serde_json::json!(["*"]));
}
