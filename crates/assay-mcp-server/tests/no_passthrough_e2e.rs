//! E6a.3 no-pass-through E2E test.
//!
//! Invariant: inbound auth (e.g. from initialize params) must never appear on any outbound
//! HTTP request. We send inbound auth, trigger the test-only outbound call, then assert
//! the mock received no sensitive headers. Run with: cargo test -p assay-mcp-server --features test-outbound no_passthrough

use assay_mcp_server::auth::SENSITIVE_HEADER_NAMES;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sensitive_names_lower() -> std::collections::HashSet<String> {
    SENSITIVE_HEADER_NAMES
        .iter()
        .map(|s| s.to_lowercase())
        .collect()
}

/// Audit-grade failure: which header names were leaked (values not logged).
fn assert_no_sensitive_headers(requests: &[wiremock::Request]) {
    let sensitive = sensitive_names_lower();
    for (i, req) in requests.iter().enumerate() {
        let mut leaked: Vec<String> = req
            .headers
            .iter()
            .filter(|(name, _)| sensitive.contains(&name.as_str().to_lowercase()))
            .map(|(name, _)| name.as_str().to_lowercase())
            .collect();
        if !leaked.is_empty() {
            let mut received_names: Vec<String> = req
                .headers
                .iter()
                .map(|(name, _)| name.as_str().to_lowercase())
                .collect();
            received_names.sort();
            leaked.sort();
            panic!(
                "E6a.3 no-pass-through violated: request #{} contained sensitive header(s) \
                 that must not be forwarded: [{}]. Received header names (values redacted): [{}].",
                i + 1,
                leaked.join(", "),
                received_names.join(", ")
            );
        }
    }
}

#[tokio::test]
async fn test_no_passthrough_e2e() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let policy_root = "../../tests/fixtures/mcp";
    let outbound_url = mock_server.uri();

    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            "assay-mcp-server",
            "--features",
            "test-outbound",
        ])
        .status()
        .expect("Failed to build server");
    assert!(status.success(), "Build with test-outbound must succeed");

    let mut child = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "assay-mcp-server",
            "--features",
            "test-outbound",
            "--",
            "--policy-root",
            policy_root,
        ])
        .env("ASSAY_TEST_OUTBOUND_URL", outbound_url.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    // 1. Initialize with inbound auth + multiple sensitive params (server must not forward any to downstream)
    let req_init = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" },
            "authorization": "Bearer INBOUND_TOKEN_NEVER_FORWARD",
            "x-api-key": "INBOUND_X_API_KEY_NEVER_FORWARD",
            "cookie": "session=INBOUND_COOKIE_NEVER_FORWARD",
            "x-forwarded-authorization": "Bearer INBOUND_FWD_AUTH_NEVER_FORWARD"
        },
        "id": 1
    });
    writeln!(stdin, "{}", req_init).unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    if reader.read_line(&mut line).unwrap() == 0 {
        reader.read_line(&mut line).unwrap();
    }
    let resp: serde_json::Value = serde_json::from_str(line.trim()).expect("Parse init response");
    assert!(resp.get("result").is_some(), "Init failed: {:?}", resp);

    // 2. Call test-only outbound tool (single callsite uses build_downstream_headers only)
    let req_call = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": { "name": "assay_test_outbound", "arguments": {} },
        "id": 2
    });
    writeln!(stdin, "{}", req_call).unwrap();
    stdin.flush().unwrap();

    line.clear();
    reader.read_line(&mut line).unwrap();
    let resp: serde_json::Value = serde_json::from_str(line.trim()).expect("Parse tool response");
    assert!(resp.get("result").is_some(), "Tool call failed: {:?}", resp);

    drop(stdin);
    let _ = child.wait();

    let received = mock_server.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        1,
        "expected exactly one outbound request (tool must not have skipped; check ASSAY_TEST_OUTBOUND_URL)"
    );
    assert_no_sensitive_headers(&received);
}
