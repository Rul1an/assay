//! Edge-case coverage for the stdio transport.
//!
//! Both spawn helpers below run the binary Cargo already built for this test target, via
//! `CARGO_BIN_EXE_assay-mcp-server`, rather than shelling out to `cargo run`. A nested Cargo
//! inherits this process's CARGO_MANIFEST_DIR, which ring's build script tracks, so it marks the
//! rustls/reqwest stack dirty every time it alternates with a shell build — 62.5s to 63.9s per
//! test, against the 120s kill budget `.config/nextest.toml` sets via
//! `slow-timeout = { period = "60s", terminate-after = 2 }`. Measured on 0799ab8b, rustc 1.96.0,
//! aarch64-apple-darwin, two back-to-back `cargo nextest run -p assay-mcp-server` with nothing
//! changed between them; the second run recompiled 14 crates before the tests even started.

use serde_json::Value;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

fn spawn_server() -> Conn {
    let policy_root = "../../tests/fixtures/mcp";
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", policy_root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    Conn::attach(child)
}

fn spawn_server_with_env(env_key: &str, env_val: &str) -> Conn {
    let policy_root = "../../tests/fixtures/mcp";
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", policy_root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env(env_key, env_val)
        .spawn()
        .expect("Failed to spawn server");

    Conn::attach(child)
}

#[test]
fn test_edge_cases() {
    let mut conn = spawn_server();

    // 1. Initialize
    conn.request(
        "initialize",
        serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}),
        1,
    );

    // Case 1: Missing Policy File (check_args)
    let resp = conn.request(
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
    // Should be ToolError: error.code == E_POLICY_NOT_FOUND
    let res = resp.get("result").expect("Valid result expected");
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    if let Some(err) = tool_res.get("error") {
        assert_eq!(
            err["code"], "E_POLICY_NOT_FOUND",
            "Should report E_POLICY_NOT_FOUND"
        );
    } else {
        panic!("Expected result.error, got result: {:?}", tool_res);
    }

    // Case 2: Malformed Policy File (check_args)
    let resp = conn.request(
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
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    assert_eq!(
        tool_res.get("allowed").and_then(|v| v.as_bool()),
        Some(false),
        "Should explicitly allow: false"
    );
    let err = tool_res
        .get("error")
        .expect("Should have error field in result");
    assert_eq!(
        err.get("code").and_then(|s| s.as_str()),
        Some("E_POLICY_PARSE"),
        "Code should be E_POLICY_PARSE"
    );

    // Case 3: Strict Schema Violation (check_args)
    let resp = conn.request(
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
    let res = resp.get("result").expect("Valid result expected");
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    let violations = tool_res["violations"]
        .as_array()
        .expect("Strict case should return violations, not error");
    // Additional properties not allowed
    assert!(
        violations.iter().any(|v| {
            let s = v["message"].as_str().unwrap_or("").to_lowercase();
            s.contains("additional") || s.contains("extra")
        }),
        "Should fail additional props. Got: {:?}",
        violations
    );

    // Case 4: Sequence - First tool requires predecessor
    let resp = conn.request(
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
    let res = resp.get("result").expect("Valid result expected");
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    assert_eq!(
        tool_res["allowed"], false,
        "Should deny action without init"
    );

    // Case 5: Policy Decide - Partial match (Security check)
    // blocklist has "dangerous_tool". "dangerous_tool_suffix" should be allowed?
    let resp = conn.request(
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
    let res = resp.get("result").expect("Valid result expected");
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    assert_eq!(
        tool_res["allowed"], true,
        "Should allow partial match if exact match is required"
    );

    let _ = conn.shutdown();
}

#[test]
fn test_timeout() {
    // Set extremely short timeout (1ms)
    let mut conn = spawn_server_with_env("ASSAY_MCP_TIMEOUT_MS", "1");

    // Initialize
    conn.request(
        "initialize",
        serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}),
        1,
    );

    // Call check_args which involves IO (policy read) - should timeout
    let resp = conn.request(
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
    // Expect E_TIMEOUT
    let res = resp.get("result").expect("Valid result expected");
    let content = res["content"][0]["text"].as_str().expect("text content");
    let tool_res: Value = serde_json::from_str(content).unwrap();

    if let Some(err) = tool_res.get("error") {
        assert_eq!(err["code"], "E_TIMEOUT", "Should report E_TIMEOUT");
    } else {
        panic!(
            "Expected timeout error, got result success/other: {:?}",
            tool_res
        );
    }

    let _ = conn.shutdown();
}
