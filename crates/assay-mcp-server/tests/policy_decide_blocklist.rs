//! Real stdio `tools/call` contract for `assay_policy_decide` blocklist parsing.
//!
//! A present `blocklist` that is not a string sequence must fail closed
//! (`allowed: false`, `E_POLICY_PARSE`, `isError: true`) and must not be
//! cached as an empty list. Absent and `[]` remain allow; a valid string
//! list still denies.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

fn spawn_server(policy_root: &Path) -> Conn {
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "--policy-root",
            policy_root.to_str().expect("utf-8 policy-root"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay-mcp-server");
    Conn::attach(child)
}

fn initialize(conn: &mut Conn) {
    conn.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "policy-decide-blocklist", "version": "1.0"}
        }),
        1,
    );
}

fn call_policy_decide(conn: &mut Conn, policy: &str, id: u64) -> (Value, Value) {
    let resp = conn.request(
        "tools/call",
        serde_json::json!({
            "name": "assay_policy_decide",
            "arguments": {
                "tool": "dangerous_tool",
                "policy": policy
            }
        }),
        id,
    );
    let result = resp
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("tools/call {id} missing result: {resp}"));
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call {id} missing text: {result}"));
    let body: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tools/call {id} body is not JSON ({e}): {text}"));
    (result, body)
}

fn assert_parse_error(label: &str, result: &Value, body: &Value) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "{label}: MCP isError must be true; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "{label}: allowed must be false; body={body}"
    );
    assert_eq!(
        body.pointer("/error/code").and_then(Value::as_str),
        Some("E_POLICY_PARSE"),
        "{label}: error.code must be E_POLICY_PARSE; body={body}"
    );
}

fn assert_allowed(label: &str, result: &Value, body: &Value) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(false),
        "{label}: MCP isError must be false; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(true),
        "{label}: allowed must be true; body={body}"
    );
    assert!(
        body.get("error").is_none(),
        "{label}: allow path must not carry error; body={body}"
    );
}

fn write_policy(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

#[test]
fn present_malformed_blocklist_fails_closed_and_is_not_cached() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();

    write_policy(root, "string.yaml", "blocklist: dangerous_tool\n");
    write_policy(root, "mapping.yaml", "blocklist:\n  dangerous_tool: true\n");
    write_policy(root, "number.yaml", "blocklist: 7\n");
    write_policy(root, "null.yaml", "blocklist: null\n");
    write_policy(root, "bool.yaml", "blocklist: true\n");
    write_policy(
        root,
        "mixed.yaml",
        "blocklist:\n  - dangerous_tool\n  - 7\n",
    );
    write_policy(root, "absent.yaml", "version: 1\n");
    write_policy(root, "empty.yaml", "blocklist: []\n");
    write_policy(root, "deny.yaml", "blocklist:\n  - dangerous_tool\n");

    let mut conn = spawn_server(root);
    initialize(&mut conn);

    let malformed = [
        "string.yaml",
        "mapping.yaml",
        "number.yaml",
        "null.yaml",
        "bool.yaml",
        "mixed.yaml",
    ];
    let mut id = 2;
    for policy in malformed {
        let (first_result, first_body) = call_policy_decide(&mut conn, policy, id);
        id += 1;
        assert_parse_error(&format!("{policy} first call"), &first_result, &first_body);

        let (second_result, second_body) = call_policy_decide(&mut conn, policy, id);
        id += 1;
        assert_parse_error(
            &format!("{policy} repeat call"),
            &second_result,
            &second_body,
        );
    }

    let (absent_result, absent_body) = call_policy_decide(&mut conn, "absent.yaml", id);
    id += 1;
    assert_allowed("absent blocklist", &absent_result, &absent_body);

    let (empty_result, empty_body) = call_policy_decide(&mut conn, "empty.yaml", id);
    id += 1;
    assert_allowed("empty blocklist []", &empty_result, &empty_body);

    let (deny_result, deny_body) = call_policy_decide(&mut conn, "deny.yaml", id);
    assert_eq!(
        deny_result.get("isError").and_then(Value::as_bool),
        Some(true),
        "valid [dangerous_tool] must set isError; result={deny_result} body={deny_body}"
    );
    assert_eq!(
        deny_body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "valid [dangerous_tool] must deny; body={deny_body}"
    );
    let matches = deny_body["matches"]
        .as_array()
        .expect("valid deny should return matches");
    assert!(
        matches.iter().any(|m| m
            .as_str()
            .is_some_and(|s| s.contains("dangerous_tool") && s.contains("blocked"))),
        "valid deny matches missing blocked-tool text: {deny_body}"
    );

    let _ = conn.shutdown();
}
