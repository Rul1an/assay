//! Wire-level contract for failures outside an individual MCP tool implementation.

use serde_json::Value;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const HEAD: &str = "HEAD_SENTINEL";
const MID: &str = "MID_SENTINEL";
const TAIL: &str = "TAIL_SENTINEL";

fn spawn_server(timeout_ms: Option<&str>) -> (Conn, tempfile::TempDir) {
    let policy_root = tempfile::tempdir().expect("temporary policy root");
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    command
        .args(["--policy-root", policy_root.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(timeout_ms) = timeout_ms {
        command.env("ASSAY_MCP_TIMEOUT_MS", timeout_ms);
    }
    let child = command.spawn().expect("spawn assay-mcp-server");
    (Conn::attach(child), policy_root)
}

fn initialize(conn: &mut Conn) {
    let response = conn.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "outer-fallback-contract", "version": "1"}
        }),
        1,
    );
    assert!(response.get("result").is_some(), "initialize: {response}");
}

fn call_tool(conn: &mut Conn, name: &str, arguments: Value, id: u64) -> Value {
    conn.request(
        "tools/call",
        serde_json::json!({"name": name, "arguments": arguments}),
        id,
    )
}

fn tool_payload(response: &Value) -> Value {
    let result = response.get("result").expect("CallToolResult response");
    let text = result["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str(text).expect("JSON tool payload")
}

fn assert_fixed_failure(response: &Value, code: &str, message: &str) {
    let result = response.get("result").expect("CallToolResult response");
    assert_eq!(result["isError"], true, "response: {response}");
    let payload = tool_payload(response);
    assert_eq!(payload["allowed"], false, "payload: {payload}");
    assert_eq!(payload["error"]["code"], code, "payload: {payload}");
    assert_eq!(payload["error"]["message"], message, "payload: {payload}");
    assert!(payload.get("warning").is_none(), "payload: {payload}");
}

#[test]
fn caller_cannot_fail_open_handler_errors() {
    let (mut conn, _root) = spawn_server(None);
    initialize(&mut conn);

    let blocked = call_tool(&mut conn, "assay_check_args", serde_json::json!({}), 2);
    let attempted_allow = call_tool(
        &mut conn,
        "assay_check_args",
        serde_json::json!({"on_error": "allow"}),
        3,
    );

    assert_fixed_failure(&blocked, "E_INTERNAL", "Tool execution failed");
    assert_fixed_failure(&attempted_allow, "E_INTERNAL", "Tool execution failed");
    assert_eq!(tool_payload(&blocked), tool_payload(&attempted_allow));
    assert!(conn.shutdown().success());
}

#[test]
fn unknown_names_are_value_free() {
    let (mut conn, _root) = spawn_server(None);
    initialize(&mut conn);
    let hostile = format!(
        "{HEAD}{}\u{1f642}{MID}{}{TAIL}",
        "x".repeat(5_000),
        "y".repeat(5_000)
    );

    let unknown_tool = call_tool(&mut conn, &hostile, serde_json::json!({}), 2);
    assert_fixed_failure(&unknown_tool, "E_INTERNAL", "Tool execution failed");

    let unknown_method = conn.request(&hostile, serde_json::json!({}), 3);
    assert_eq!(unknown_method["error"]["code"], -32601);
    assert_eq!(unknown_method["error"]["message"], "Method not found");

    for response in [&unknown_tool, &unknown_method] {
        let wire = serde_json::to_string(response).unwrap();
        for sentinel in [HEAD, MID, TAIL] {
            assert!(!wire.contains(sentinel), "response reflected {sentinel}");
        }
    }
    assert!(conn.shutdown().success());
}

#[test]
fn caller_cannot_fail_open_timeouts() {
    let (mut conn, _root) = spawn_server(Some("1"));
    initialize(&mut conn);

    let response = call_tool(
        &mut conn,
        "assay_check_args",
        serde_json::json!({
            "tool": "any",
            "arguments": {},
            "policy": "slow.yaml",
            "on_error": "allow"
        }),
        2,
    );

    assert_fixed_failure(&response, "E_TIMEOUT", "Tool execution timed out");
    assert!(conn.shutdown().success());
}
