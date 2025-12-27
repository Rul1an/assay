use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn spawn_server() -> (
    std::process::Child,
    std::process::ChildStdin,
    BufReader<std::process::ChildStdout>,
) {
    let policy_root = "../../tests/fixtures/mcp";
    let mut child = Command::new("cargo")
        .args(&[
            "run",
            "-q",
            "-p",
            "assay-mcp-server",
            "--",
            "--policy-root",
            policy_root,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    let stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    return (child, stdin, BufReader::new(stdout));
}

fn spawn_server_with_env(
    env_key: &str,
    env_val: &str,
) -> (
    std::process::Child,
    std::process::ChildStdin,
    BufReader<std::process::ChildStdout>,
) {
    let policy_root = "../../tests/fixtures/mcp";
    let mut child = Command::new("cargo")
        .args(&[
            "run",
            "-q",
            "-p",
            "assay-mcp-server",
            "--",
            "--policy-root",
            policy_root,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env(env_key, env_val)
        .spawn()
        .expect("Failed to spawn server");

    let stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    return (child, stdin, BufReader::new(stdout));
}

fn send_req(
    stdin: &mut std::process::ChildStdin,
    reader: &mut BufReader<std::process::ChildStdout>,
    method: &str,
    params: Value,
    id: u64,
) -> Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });
    writeln!(stdin, "{}", req).unwrap();

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            panic!("Server sent EOF (crashed?) waiting for response id={}", id);
        }
        if !line.trim().is_empty() {
            // Check if it's a log line (heuristic) - currently server logs to stderr, so stdout should be pure JSON
            if let Ok(val) = serde_json::from_str::<Value>(&line) {
                return val;
            }
        }
    }
}

#[test]
fn test_edge_cases() {
    let (mut child, mut stdin, mut reader) = spawn_server();

    // 1. Initialize
    send_req(
        &mut stdin,
        &mut reader,
        "initialize",
        serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}),
        1,
    );

    // Case 1: Missing Policy File (check_args)
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_check_args",
            "arguments": {
                "tool": "any",
                "arguments": {},
                "policy": "non_existent.yaml"
            }
        }),
        2,
    );
    // Should be ToolError: error.code == E_POLICY_NOT_FOUND
    if let Some(res) = resp.get("result") {
        if let Some(err) = res.get("error") {
            assert_eq!(
                err["code"], "E_POLICY_NOT_FOUND",
                "Should report E_POLICY_NOT_FOUND"
            );
        } else {
            panic!("Expected result.error, got result: {:?}", res);
        }
    } else {
        panic!("Expected result object, got: {:?}", resp);
    }

    // Case 2: Malformed Policy File (check_args)
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_check_args",
            "arguments": {
                "tool": "any",
                "arguments": {},
                "policy": "malformed.yaml"
            }
        }),
        3,
    );
    // Expect error or violation? Current impl returns Ok with violations or error?
    // check_args.rs now captures the error and returns it in "result"
    // so we expect result.allowed=false and result.error.code=E_POLICY_PARSE
    assert!(resp.get("result").is_some());
    let res = resp.get("result").unwrap();
    assert_eq!(
        res.get("allowed").and_then(|v| v.as_bool()),
        Some(false),
        "Should explicitly allow: false"
    );
    let err = res.get("error").expect("Should have error field in result");
    assert_eq!(
        err.get("code").and_then(|s| s.as_str()),
        Some("E_POLICY_PARSE"),
        "Code should be E_POLICY_PARSE"
    );

    // Case 3: Strict Schema Violation (check_args)
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_check_args",
            "arguments": {
                "tool": "strict_tool",
                "arguments": { "code": 123, "extra": "field" },
                "policy": "strict_policy.yaml"
            }
        }),
        4,
    );
    let violations = resp["result"]["violations"]
        .as_array()
        .expect("Strict case should return violations, not error");
    // Additional properties not allowed
    assert!(
        violations.iter().any(|v| {
            let s = v["constraint"].as_str().unwrap_or("").to_lowercase();
            s.contains("additional") || s.contains("extra")
        }),
        "Should fail additional props. Got: {:?}",
        violations
    );

    // Case 4: Sequence - First tool requires predecessor
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_check_sequence",
            "arguments": {
                "history": [],
                "next_tool": "action", // requires 'init' from previous fixture
                "policy": "sequence_policy.yaml"
            }
        }),
        5,
    );
    assert_eq!(
        resp["result"]["allowed"], false,
        "Should deny action without init"
    );

    // Case 5: Policy Decide - Partial match (Security check)
    // blocklist has "dangerous_tool". "dangerous_tool_suffix" should be allowed?
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_policy_decide",
            "arguments": {
                "tool": "dangerous_tool_suffix",
                "policy": "blocklist_policy.yaml"
            }
        }),
        6,
    );
    assert_eq!(
        resp["result"]["allowed"], true,
        "Should allow partial match if exact match is required"
    );

    drop(stdin);
    let _ = child.wait();
}

#[test]
fn test_timeout() {
    // Set extremely short timeout (1ms)
    let (mut child, mut stdin, mut reader) = spawn_server_with_env("ASSAY_MCP_TIMEOUT_MS", "1");

    // Initialize
    send_req(
        &mut stdin,
        &mut reader,
        "initialize",
        serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}),
        1,
    );

    // Call check_args which involves IO (policy read) - should timeout
    let resp = send_req(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "assay_check_args",
            "arguments": {
                "tool": "any",
                "arguments": {},
                "policy": "slow.yaml"
            }
        }),
        2,
    );

    // Expect E_TIMEOUT
    if let Some(res) = resp.get("result") {
        if let Some(err) = res.get("error") {
            assert_eq!(err["code"], "E_TIMEOUT", "Should report E_TIMEOUT");
        } else {
            panic!(
                "Expected timeout error, got result success/other: {:?}",
                res
            );
        }
    } else {
        panic!("Expected result object, got: {:?}", resp);
    }

    drop(stdin);
    let _ = child.wait();
}
