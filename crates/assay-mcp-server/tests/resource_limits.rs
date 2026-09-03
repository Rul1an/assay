use anyhow::Result;
use serde_json::Value;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

// Helper to spawn server with env vars
fn spawn_server_with_env(env: Vec<(&str, &str)>) -> Conn {
    let cargo_bin = env!("CARGO_BIN_EXE_assay-mcp-server");
    let mut cmd = Command::new(cargo_bin);
    // Use --policy-root flag as required by main.rs
    cmd.arg("--policy-root").arg("../../tests/fixtures/mcp");
    cmd.env_clear();
    cmd.envs(std::env::vars()); // Inherit PATH etc
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());
    Conn::attach(cmd.spawn().expect("spawn assay-mcp-server"))
}

fn send_req(conn: &mut Conn, req: Value) -> Value {
    conn.send(req);
    conn.read_json()
}

fn extract_inner(resp: &Value) -> Value {
    let result = resp.get("result").expect("Missing result");
    // New MCP compliance wrapping
    if let Some(content) = result.get("content") {
        let text = content[0]
            .get("text")
            .expect("Missing text")
            .as_str()
            .expect("Not string");
        serde_json::from_str(text).expect("Failed to parse inner JSON")
    } else {
        // Fallback if unwrapped (e.g. error)
        result.clone()
    }
}

#[test]
fn test_transport_limit_exceeded() -> Result<()> {
    // Pre-parse max_msg_bytes rejection is a JSON-RPC protocol error (id null), not a
    // CallToolResult / success result. Session must stay alive for a following in-limit request.
    const LIMIT: usize = 100;
    const SENTINEL: &str = "TRANSPORT_LIMIT_HOSTILE_SENTINEL";
    let mut conn = spawn_server_with_env(vec![("ASSAY_MCP_MAX_BYTES", "100")]);

    let padding = "x".repeat(200);
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "huge": format!("{SENTINEL}{padding}") },
        "id": 1
    });
    let req_wire = serde_json::to_string(&req).expect("serialize oversize request");
    assert!(
        req_wire.len() > LIMIT,
        "fixture must exceed transport limit: len={} limit={LIMIT}",
        req_wire.len()
    );
    assert!(
        req_wire.contains(SENTINEL),
        "fixture must embed sentinel for reflection check"
    );

    let resp = send_req(&mut conn, req);
    let wire = serde_json::to_string(&resp).expect("serialize response");

    assert!(
        resp.get("result").is_none(),
        "transport limit must not use JSON-RPC result: {resp}"
    );
    let err = resp
        .get("error")
        .unwrap_or_else(|| panic!("transport limit must be top-level JSON-RPC error: {resp}"));
    assert_eq!(
        resp.get("id"),
        Some(&Value::Null),
        "pre-parse refusal must use id null: {resp}"
    );
    assert_eq!(err.get("code"), Some(&Value::from(-32000)), "{resp}");
    assert_eq!(
        err.get("message").and_then(Value::as_str),
        Some("Message too large"),
        "{resp}"
    );
    // Complete data equality: an added reflected length/body field must not survive.
    assert_eq!(
        err.get("data"),
        Some(&serde_json::json!({
            "kind": "transport_limit",
            "limit": LIMIT,
        })),
        "{resp}"
    );
    assert!(
        !wire.contains(SENTINEL),
        "response must not reflect request body sentinel: {wire}"
    );
    assert!(
        !wire.contains("E_LIMIT_EXCEEDED"),
        "transport limit must not use tool-domain E_LIMIT_EXCEEDED: {wire}"
    );

    // Same session: a following in-limit request must still be answered.
    let live = send_req(
        &mut conn,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "id": 2
        }),
    );
    assert_eq!(live.get("id"), Some(&Value::from(2)), "liveness id: {live}");
    assert!(
        live.get("error").is_none(),
        "session must continue after transport limit: {live}"
    );
    assert!(
        live.pointer("/result/tools")
            .map(Value::is_array)
            .unwrap_or(false),
        "expected tools/list result after oversize refusal: {live}"
    );

    conn.kill();
    Ok(())
}

#[test]
fn test_payload_field_limit() -> Result<()> {
    // MAX_FIELD_BYTES = 50
    let mut conn = spawn_server_with_env(vec![("ASSAY_MCP_MAX_FIELD_BYTES", "50")]);

    // Tool name len 4, OK.
    // Policy path len > 50 -> Fail.
    let long_policy = "policies/".to_string() + &"a".repeat(100) + ".yaml";

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": "test",
                "arguments": {},
                "policy": long_policy
            }
        },
        "id": 1
    });

    let resp = send_req(&mut conn, req);
    let inner = extract_inner(&resp);

    assert_eq!(inner.get("allowed").unwrap().as_bool(), Some(false));
    let code = inner
        .get("error")
        .unwrap()
        .get("code")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(code, "E_LIMIT_EXCEEDED");

    conn.kill();
    Ok(())
}

#[test]
fn test_sequence_history_limit() -> Result<()> {
    // MAX_TOOL_CALLS = 3
    let mut conn = spawn_server_with_env(vec![("ASSAY_MCP_MAX_TOOL_CALLS", "3")]);

    // History with 3 calls (OK)
    let history_ok: Vec<String> = vec!["tool_a".into(), "tool_b".into(), "tool_c".into()];
    let req_ok = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_sequence",
            "arguments": {
                "history": history_ok,
                "next_tool": "tool_d",
                "policy": "sequence_policy.yaml"
            }
        },
        "id": 1
    });

    // History with 4 calls (Fail)
    let history_fail: Vec<String> = vec![
        "tool_a".into(),
        "tool_b".into(),
        "tool_c".into(),
        "tool_d".into(),
    ];
    let req_fail = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_sequence",
            "arguments": {
                "history": history_fail,
                "next_tool": "tool_e",
                "policy": "sequence_policy.yaml"
            }
        },
        "id": 2
    });

    let resp_ok = send_req(&mut conn, req_ok);
    let inner_ok = extract_inner(&resp_ok);

    // It might fail with policy error (not found), but NOT limit error.
    if let Some(err) = inner_ok.get("error") {
        assert_ne!(
            err.get("code").unwrap().as_str().unwrap(),
            "E_LIMIT_EXCEEDED"
        );
    }

    let resp_fail = send_req(&mut conn, req_fail);
    let inner_fail = extract_inner(&resp_fail);

    assert_eq!(inner_fail.get("allowed").unwrap().as_bool(), Some(false));
    let code = inner_fail
        .get("error")
        .unwrap()
        .get("code")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(code, "E_LIMIT_EXCEEDED");

    conn.kill();
    Ok(())
}

#[test]
fn test_boundary_exact_limits() -> Result<()> {
    // MAX_FIELD_BYTES = 10
    let mut conn = spawn_server_with_env(vec![("ASSAY_MCP_MAX_FIELD_BYTES", "10")]);

    // 10 bytes (OK)
    let tool_name = "1234567890";
    let req_ok = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": tool_name,
                "arguments": {},
                "policy": "short.yaml"
            }
        },
        "id": 1
    });

    // 11 bytes (Fail)
    let tool_name_fail = "12345678901";
    let req_fail = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": tool_name_fail,
                "arguments": {},
                "policy": "short.yaml"
            }
        },
        "id": 2
    });

    let resp_ok = send_req(&mut conn, req_ok);
    let inner_ok = extract_inner(&resp_ok);

    // Might fail policy read, but NOT limit
    if let Some(err) = inner_ok.get("error") {
        assert_ne!(
            err.get("code").unwrap().as_str().unwrap(),
            "E_LIMIT_EXCEEDED",
            "10 bytes should pass limit check"
        );
    }

    let resp_fail = send_req(&mut conn, req_fail);
    let inner_fail = extract_inner(&resp_fail);

    assert_eq!(inner_fail.get("allowed").unwrap().as_bool(), Some(false));
    assert_eq!(
        inner_fail
            .get("error")
            .unwrap()
            .get("code")
            .unwrap()
            .as_str()
            .unwrap(),
        "E_LIMIT_EXCEEDED"
    );

    conn.kill();
    Ok(())
}
