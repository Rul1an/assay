#![allow(deprecated)]

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn coverage_routes_top_limits_markdown_route_rows() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.jsonl");
    let out_json = dir.path().join("coverage.json");
    let out_md = dir.path().join("coverage.md");

    let input_jsonl = [
        r#"{"tool":"A"}"#,
        r#"{"tool":"B"}"#,
        r#"{"tool":"A"}"#,
        r#"{"tool":"B"}"#,
        r#"{"tool":"A"}"#,
        r#"{"tool":"B"}"#,
        r#"{"tool":"A"}"#,
        r#"{"tool":"B"}"#,
        r#"{"tool":"A"}"#,
        r#"{"tool":"B"}"#,
        r#"{"tool":"C"}"#,
        r#"{"tool":"D"}"#,
    ]
    .join("\n");
    fs::write(&input, input_jsonl).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args(["coverage", "--input"])
        .arg(&input)
        .args(["--out"])
        .arg(&out_json)
        .args(["--out-md"])
        .arg(&out_md)
        .args(["--routes-top", "1"])
        .assert()
        .success();

    let markdown = fs::read_to_string(out_md).expect("markdown should exist");
    assert!(markdown.contains("| `A` | `B` | 5 |"));
    assert!(!markdown.contains("| `B` | `A` | 4 |"));
}
