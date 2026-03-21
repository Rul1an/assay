use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
#[allow(deprecated)]
fn contract_import_rejects_boolean_jsonrpc_id() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("bool-id.jsonl");
    fs::write(
        &input,
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":true,"method":"tools/call","params":{"name":"BoolId","arguments":{"x":1}}}
"#,
    )
    .expect("write input");

    let assert = Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("import")
        .arg(&input)
        .arg("--format")
        .arg("jsonrpc")
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("failed to parse MCP transcript"),
        "missing parse context: {stderr}"
    );
    assert!(
        stderr.contains("must not be a boolean"),
        "missing boolean-id diagnostic: {stderr}"
    );
}

#[test]
#[allow(deprecated)]
fn contract_import_rejects_duplicate_tool_call_request_ids() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("duplicate-id.jsonl");
    fs::write(
        &input,
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"dup-1","method":"tools/call","params":{"name":"First","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"dup-1","method":"tools/call","params":{"name":"Second","arguments":{"x":2}}}
"#,
    )
    .expect("write input");

    let assert = Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("import")
        .arg(&input)
        .arg("--format")
        .arg("jsonrpc")
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("duplicate tools/call request id"),
        "missing duplicate-id diagnostic: {stderr}"
    );
}
