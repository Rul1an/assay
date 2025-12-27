use anyhow::Result;
use serde_json::Value;
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

// Helper to spawn server with env vars
fn spawn_server_with_env(env: Vec<(&str, &str)>) -> Result<std::process::Child> {
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
    Ok(cmd.spawn()?)
}

fn send_req(child: &mut std::process::Child, req: Value) -> Result<Value> {
    let stdin = child.stdin.as_mut().unwrap();
    let line = serde_json::to_string(&req)?;
    writeln!(stdin, "{}", line)?;

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: Value = serde_json::from_str(&line)?;
    Ok(resp)
}

#[test]
fn test_transport_limit_exceeded() -> Result<()> {
    // MAX_BYTES = 100
    let mut child = spawn_server_with_env(vec![("ASSAY_MCP_MAX_BYTES", "100")])?;

    // Create huge request
    let huge_params = "x".repeat(200);
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "huge": huge_params },
        "id": 1
    });

    let resp = send_req(&mut child, req)?;
    // Expect allowed: false, error code E_LIMIT_EXCEEDED (transport level)

    let result = resp.get("result").unwrap();
    let allowed = result.get("allowed").unwrap().as_bool().unwrap();
    assert_eq!(allowed, false);

    let err = result.get("error").unwrap();
    assert_eq!(
        err.get("code").unwrap().as_str().unwrap(),
        "E_LIMIT_EXCEEDED"
    );

    child.kill()?;
    Ok(())
}

#[test]
fn test_payload_field_limit() -> Result<()> {
    // MAX_FIELD_BYTES = 50
    let mut child = spawn_server_with_env(vec![("ASSAY_MCP_MAX_FIELD_BYTES", "50")])?;

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

    let resp = send_req(&mut child, req)?;
    let result = resp.get("result").unwrap();
    assert_eq!(result.get("allowed").unwrap().as_bool(), Some(false));
    let code = result
        .get("error")
        .unwrap()
        .get("code")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(code, "E_LIMIT_EXCEEDED");

    child.kill()?;
    Ok(())
}

#[test]
fn test_sequence_history_limit() -> Result<()> {
    // MAX_TOOL_CALLS = 3
    let mut child = spawn_server_with_env(vec![("ASSAY_MCP_MAX_TOOL_CALLS", "3")])?;

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

    let resp_ok = send_req(&mut child, req_ok)?;
    // It might fail with policy error, but NOT limit error.
    let res_ok = resp_ok.get("result").unwrap();
    if let Some(err) = res_ok.get("error") {
        assert_ne!(
            err.get("code").unwrap().as_str().unwrap(),
            "E_LIMIT_EXCEEDED"
        );
    }

    let resp_fail = send_req(&mut child, req_fail)?;
    let res_fail = resp_fail.get("result").unwrap();
    assert_eq!(res_fail.get("allowed").unwrap().as_bool(), Some(false));
    let code = res_fail
        .get("error")
        .unwrap()
        .get("code")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(code, "E_LIMIT_EXCEEDED");

    child.kill()?;
    Ok(())
}

#[test]
fn test_boundary_exact_limits() -> Result<()> {
    // MAX_FIELD_BYTES = 10
    let mut child = spawn_server_with_env(vec![("ASSAY_MCP_MAX_FIELD_BYTES", "10")])?;

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

    let resp_ok = send_req(&mut child, req_ok)?;
    // Might fail policy read, but NOT limit
    if let Some(res) = resp_ok.get("result") {
        if let Some(err) = res.get("error") {
            assert_ne!(
                err.get("code").unwrap().as_str().unwrap(),
                "E_LIMIT_EXCEEDED",
                "10 bytes should pass limit check"
            );
        }
    }

    let resp_fail = send_req(&mut child, req_fail)?;
    let res_fail = resp_fail.get("result").unwrap();
    assert_eq!(res_fail.get("allowed").unwrap().as_bool(), Some(false));
    assert_eq!(
        res_fail
            .get("error")
            .unwrap()
            .get("code")
            .unwrap()
            .as_str()
            .unwrap(),
        "E_LIMIT_EXCEEDED"
    );

    child.kill()?;
    Ok(())
}
