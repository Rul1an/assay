//! Wire-level contract for failures outside an individual MCP tool implementation.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const HEAD: &str = "HEAD_SENTINEL";
const MID: &str = "MID_SENTINEL";
const TAIL: &str = "TAIL_SENTINEL";
const NOT_JSON: &str = "NOT_JSON_SENTINEL";

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
fn report_tools_keep_successful_mcp_results() {
    let (mut conn, root) = spawn_server(None);
    fs::write(
        root.path().join("policy.yaml"),
        "version: \"1.1\"\nname: report-tools\ntools:\n  allow: [Search]\nsequences: []\n",
    )
    .expect("write report policy");
    initialize(&mut conn);

    let coverage = call_tool(
        &mut conn,
        "assay_check_coverage",
        serde_json::json!({
            "policy": "policy.yaml",
            "traces": [{"tools": ["Search"]}]
        }),
        2,
    );
    let explanation = call_tool(
        &mut conn,
        "assay_explain_trace",
        serde_json::json!({
            "policy": "policy.yaml",
            "trace": [{"tool": "Search"}]
        }),
        3,
    );

    for response in [&coverage, &explanation] {
        assert_eq!(response["result"]["isError"], false, "response: {response}");
        assert!(tool_payload(response).get("error").is_none());
    }
    assert!(conn.shutdown().success());
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

fn assert_tools_call_protocol_fault(response: &Value, id: u64, kind: &str, message: &str) {
    assert!(
        response.get("result").is_none(),
        "protocol fault must not use JSON-RPC result: {response}"
    );
    assert_eq!(response.get("id"), Some(&Value::from(id)), "{response}");
    let err = response
        .get("error")
        .unwrap_or_else(|| panic!("expected top-level JSON-RPC error: {response}"));
    assert_eq!(err.get("code"), Some(&Value::from(-32602)), "{response}");
    assert_eq!(
        err.get("message").and_then(Value::as_str),
        Some(message),
        "{response}"
    );
    assert_eq!(
        err.get("data"),
        Some(&serde_json::json!({ "kind": kind })),
        "complete data equality: {response}"
    );
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
    assert_tools_call_protocol_fault(&unknown_tool, 2, "unknown_tool", "Unknown tool");

    let unknown_method = conn.request(&hostile, serde_json::json!({}), 3);
    assert_eq!(unknown_method["error"]["code"], -32601);
    assert_eq!(unknown_method["error"]["message"], "Method not found");
    assert!(
        unknown_method
            .get("error")
            .and_then(|e| e.get("data"))
            .is_none(),
        "method-not-found stays data-free: {unknown_method}"
    );

    for response in [&unknown_tool, &unknown_method] {
        let wire = serde_json::to_string(response).unwrap();
        for sentinel in [HEAD, MID, TAIL] {
            assert!(!wire.contains(sentinel), "response reflected {sentinel}");
        }
        assert!(
            !wire.contains("E_INTERNAL"),
            "unknown tool must not collapse to E_INTERNAL: {wire}"
        );
    }
    assert!(conn.shutdown().success());
}

#[test]
fn tools_call_envelope_faults_are_distinct_protocol_errors() {
    let (mut conn, _root) = spawn_server(None);
    initialize(&mut conn);

    // Missing params object entirely.
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "id": 10
    }));
    let missing = conn.read_response();
    assert_tools_call_protocol_fault(&missing, 10, "malformed_call", "Invalid params");

    // Non-object params.
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": 1,
        "id": 11
    }));
    let non_object = conn.read_response();
    assert_tools_call_protocol_fault(&non_object, 11, "malformed_call", "Invalid params");

    // Object params but name missing / wrong type.
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "arguments": {} },
        "id": 12
    }));
    let missing_name = conn.read_response();
    assert_tools_call_protocol_fault(&missing_name, 12, "malformed_call", "Invalid params");

    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "name": 7, "arguments": {} },
        "id": 13
    }));
    let bad_name_type = conn.read_response();
    assert_tools_call_protocol_fault(&bad_name_type, 13, "malformed_call", "Invalid params");

    // arguments present but not an object.
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "name": "assay_check_args", "arguments": "nope" },
        "id": 14
    }));
    let bad_args = conn.read_response();
    assert_tools_call_protocol_fault(&bad_args, 14, "malformed_call", "Invalid params");

    // Unknown tool stays a distinct kind from malformed_call.
    let unknown = call_tool(&mut conn, "not_a_real_tool", serde_json::json!({}), 15);
    assert_tools_call_protocol_fault(&unknown, 15, "unknown_tool", "Unknown tool");
    assert_ne!(
        unknown["error"]["data"], missing["error"]["data"],
        "unknown_tool and malformed_call must remain distinct"
    );

    // Selected known tool with input-domain failure remains CallToolResult isError.
    let selected = call_tool(&mut conn, "assay_check_args", serde_json::json!({}), 16);
    assert_fixed_failure(&selected, "E_INTERNAL", "Tool execution failed");

    for response in [
        &missing,
        &non_object,
        &missing_name,
        &bad_name_type,
        &bad_args,
        &unknown,
    ] {
        let wire = serde_json::to_string(response).unwrap();
        assert!(
            !wire.contains("E_INTERNAL"),
            "envelope faults must not use tool E_INTERNAL: {wire}"
        );
        assert!(
            response.pointer("/result/isError").is_none(),
            "envelope faults must not be CallToolResult: {response}"
        );
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

/// Batch stdin with piped stderr. `Conn::send` only accepts JSON `Value`, so a
/// non-JSON line has to go through this local spawn, not the shared harness.
fn run_raw_session(lines: &[&str]) -> Output {
    let policy_root = tempfile::tempdir().expect("temporary policy root");
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", policy_root.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assay-mcp-server");
    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for line in lines {
            writeln!(stdin, "{line}").expect("write stdin line");
        }
    }
    let output = child.wait_with_output().expect("wait for server");
    drop(policy_root);
    output
}

#[test]
fn invalid_json_line_is_ignored_and_session_continues() {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "outer-fallback-contract", "version": "1"}
        }
    });
    let output = run_raw_session(&[NOT_JSON, &initialize.to_string()]);
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains(NOT_JSON), "stdout reflected {NOT_JSON}");
    let responses: Vec<Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect();
    assert_eq!(responses.len(), 1, "stdout: {stdout}");
    assert_eq!(responses[0]["id"], 1);
    assert!(
        responses[0].get("result").is_some(),
        "initialize: {}",
        responses[0]
    );
    assert!(
        responses[0].get("error").is_none(),
        "unexpected protocol error: {}",
        responses[0]
    );
    for response in &responses {
        assert_ne!(
            response.pointer("/error/code"),
            Some(&serde_json::json!(-32700)),
            "parse error leaked: {response}"
        );
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(NOT_JSON), "stderr reflected {NOT_JSON}");
}
