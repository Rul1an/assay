//! E6a.3 no-pass-through E2E test.
//!
//! Invariant: credential-shaped initialize fields are not authentication on standalone stdio and
//! must never appear on any Assay-originated outbound HTTP request. We send a sentinel field,
//! trigger the test-only outbound call, then assert the mock received no sensitive headers or
//! sentinel values. Run with: cargo test -p assay-mcp-server --features test-outbound no_passthrough
//!
//! This file is gated on `test-outbound` because `CARGO_BIN_EXE_assay-mcp-server` names one
//! uplift slot, not a per-feature one — `target/debug/assay-mcp-server` for the usual dev build,
//! relocated by `--target` or a non-dev profile and suffixed `.exe` on Windows — and its contents
//! are whichever variant Cargo last put there. The gate is what makes the invocation that runs
//! this test also be one that asked for the feature, so the variant Cargo uplifts on the way in
//! is the one the test needs.
//!
//! That is narrower than it may read, and the narrowness was measured rather than assumed: Cargo
//! releases the build lock when compilation finishes, before test binaries execute, so a
//! concurrent `cargo build -p assay-mcp-server` in another shell can re-uplift the feature-less
//! variant mid-run. Reproduced; the test then fails on `received.len() == 1` below, because a
//! binary without the feature has no `assay_test_outbound` to call. It fails closed, never green,
//! but it is a real flake if you build this crate in a second terminal while this test runs.
//!
//! Previously the file compiled unconditionally and shelled out to `cargo build --features
//! test-outbound`, whose inherited CARGO_MANIFEST_DIR dirtied the shared stack like every other
//! nested Cargo here. Note the dependency stack itself never had two variants to thrash between:
//! `test-outbound = []` enables no dependency features, so only this crate's own units differ.
//!
//! Gating trades a rebuild for an absence, which is the more dangerous failure: `cargo test` exits
//! 0 when zero tests match. So the CI job that owns this invariant enables the feature and asserts
//! the test actually ran; see the E6a.3 step in .github/workflows/ci.yml.
//!
//! Absence has a cost worth knowing before you edit this file: no gate that omits
//! `--features test-outbound` compiles it, and none of them carry `--all-features`. That includes
//! `cargo clippy --workspace --all-targets` in CI, which is why a dedicated feature-enabled clippy
//! step sits beside the test step, but it also includes both pre-push hooks
//! (`scripts/precommit/cargo-clippy.sh`, `scripts/ci/check-linux.sh`) and a plain
//! `cargo test -p assay-mcp-server`. A syntax or type error here passes every local check and only
//! surfaces on CI. Run the documented command above before pushing changes to this file.
#![cfg(feature = "test-outbound")]

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_no_passthrough_e2e() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let policy_root = "../../tests/fixtures/mcp";
    let outbound_url = mock_server.uri();

    // This test only compiles under `test-outbound`, so the binary Cargo built for it carries the
    // feature too — no separate build step, and no second feature variant to thrash against.
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", policy_root])
        .env("ASSAY_TEST_OUTBOUND_URL", outbound_url.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    // 1. Initialize with credential-shaped fields. The server must neither interpret them as
    // authentication nor forward them downstream.
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
    for _ in 0..10 {
        line.clear();
        let n = reader.read_line(&mut line).expect("read init response");
        if n == 0 {
            break;
        }
        if !line.trim().is_empty() {
            break;
        }
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
    let status = child.wait().expect("failed to wait on child process");
    assert!(
        status.success(),
        "server process did not exit successfully: {status:?}"
    );

    let received = mock_server.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        1,
        "expected exactly one outbound request (tool must not have skipped; check ASSAY_TEST_OUTBOUND_URL)"
    );
    assert_no_sensitive_headers(&received);
    // MCP01a-3 value-sentinel: the EXACT inbound value must not re-emit on ANY outbound surface
    // (header values, URL, or body), not just be absent by header name. Every inbound field
    // value above carries the marker `NEVER_FORWARD`, so its presence anywhere outbound is a leak.
    assert_no_inbound_value_leaked(&received);
}

/// Value-sentinel proof (MCP01a-3): the inbound credential-shaped VALUE never appears on an outbound
/// surface. Checks header values, the URL, and the body, not header names. Value-free failure (the
/// surface, not the value).
fn assert_no_inbound_value_leaked(requests: &[wiremock::Request]) {
    const INBOUND_VALUE_SENTINEL: &str = "NEVER_FORWARD";
    for (i, req) in requests.iter().enumerate() {
        let mut surfaces: Vec<String> = vec![req.url.as_str().to_string()];
        for (_name, value) in req.headers.iter() {
            surfaces.push(value.to_str().unwrap_or("<binary>").to_string());
        }
        surfaces.push(String::from_utf8_lossy(&req.body).to_string());
        if surfaces.iter().any(|s| s.contains(INBOUND_VALUE_SENTINEL)) {
            panic!(
                "MCP01a-3 value passthrough violated: request #{} re-emitted an inbound \
                 credential-shaped value on an outbound surface (header value, URL, or body). \
                 Sentinel marker {INBOUND_VALUE_SENTINEL:?} found; value not logged.",
                i + 1
            );
        }
    }
}
