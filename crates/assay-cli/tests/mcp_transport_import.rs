use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
#[allow(deprecated)]
fn contract_import_streamable_http_writes_trace() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("streamable-http.json");
    let output = dir.path().join("streamable-http.trace.jsonl");
    fs::write(&input, streamable_http_fixture()).expect("write input");

    Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("import")
        .arg(&input)
        .arg("--format")
        .arg("streamable-http")
        .arg("--out-trace")
        .arg(&output)
        .assert()
        .success();

    assert_trace_contains_tools(&output, &["Calculator"]);
}

#[test]
#[allow(deprecated)]
fn contract_import_http_sse_writes_trace() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("http-sse.json");
    let output = dir.path().join("http-sse.trace.jsonl");
    fs::write(&input, http_sse_fixture()).expect("write input");

    Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("import")
        .arg(&input)
        .arg("--format")
        .arg("http-sse")
        .arg("--out-trace")
        .arg(&output)
        .assert()
        .success();

    assert_trace_contains_tools(&output, &["Calculator"]);
}

#[test]
#[allow(deprecated)]
fn contract_trace_import_mcp_streamable_http_writes_trace() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("streamable-http.json");
    let output = dir.path().join("streamable-http.trace.jsonl");
    fs::write(&input, streamable_http_fixture()).expect("write input");

    Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("trace")
        .arg("import-mcp")
        .arg("--input")
        .arg(&input)
        .arg("--out-trace")
        .arg(&output)
        .arg("--format")
        .arg("streamable-http")
        .assert()
        .success();

    assert_trace_contains_tools(&output, &["Calculator"]);
}

#[test]
#[allow(deprecated)]
fn contract_trace_import_mcp_sse_legacy_alias_writes_trace() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("http-sse.json");
    let output = dir.path().join("http-sse.trace.jsonl");
    fs::write(&input, http_sse_fixture()).expect("write input");

    Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("trace")
        .arg("import-mcp")
        .arg("--input")
        .arg(&input)
        .arg("--out-trace")
        .arg(&output)
        .arg("--format")
        .arg("sse-legacy")
        .assert()
        .success();

    assert_trace_contains_tools(&output, &["Calculator"]);
}

#[test]
#[allow(deprecated)]
fn contract_import_streamable_http_401_www_authenticate_writes_k2_auth_discovery_summary() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("streamable-http-authz.json");
    let output = dir.path().join("streamable-http-authz.trace.jsonl");
    fs::write(&input, streamable_http_authz_fixture()).expect("write input");

    Command::cargo_bin("assay")
        .expect("assay binary")
        .arg("trace")
        .arg("import-mcp")
        .arg("--input")
        .arg(&input)
        .arg("--out-trace")
        .arg(&output)
        .arg("--format")
        .arg("streamable-http")
        .assert()
        .success();

    let text = fs::read_to_string(&output).expect("read trace output");
    let episode_start: Value = serde_json::from_str(
        text.lines()
            .find(|line| line.contains("\"type\":\"episode_start\""))
            .expect("episode_start present"),
    )
    .expect("valid episode_start");

    assert_eq!(
        episode_start["meta"]["mcp"]["authorization_discovery"],
        json!({
            "visible": true,
            "source_kind": "www_authenticate",
            "resource_metadata_visible": true,
            "authorization_servers_visible": false,
            "scope_challenge_visible": true
        })
    );
}

fn assert_trace_contains_tools(path: &Path, tools: &[&str]) {
    let text = fs::read_to_string(path).expect("read trace output");
    assert!(!text.trim().is_empty(), "trace output should not be empty");

    let tool_calls: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).expect("valid trace jsonl");
            match value.get("type").and_then(Value::as_str) {
                Some("tool_call") => value
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                _ => None,
            }
        })
        .collect();

    let expected: Vec<String> = tools.iter().map(|tool| (*tool).to_string()).collect();
    assert_eq!(tool_calls, expected);
}

fn streamable_http_fixture() -> String {
    json!({
        "transport": "streamable-http",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2025-06-18"
            }
        },
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "result": { "sum": 3 }
                }
            }
        ]
    })
    .to_string()
}

fn http_sse_fixture() -> String {
    json!({
        "transport": "http-sse",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2024-11-05",
                "Mcp-Session-Id": "legacy-session"
            }
        },
        "entries": [
            {
                "timestamp_ms": 999,
                "sse": {
                    "event": "endpoint",
                    "id": "evt-0",
                    "data": "/mcp/messages?session=legacy-session"
                }
            },
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "sse": {
                    "event": "message",
                    "id": "evt-1",
                    "data": "{\"jsonrpc\":\"2.0\",\"id\":\"call-1\",\"result\":{\"sum\":3}}"
                }
            }
        ]
    })
    .to_string()
}

fn streamable_http_authz_fixture() -> String {
    json!({
        "transport": "streamable-http",
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "transport_context": {
                    "status": 401,
                    "headers": {
                        "WWW-Authenticate": "Bearer resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\", scope=\"tools/call\""
                    }
                },
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "error": { "code": 401, "message": "unauthorized" }
                }
            }
        ]
    })
    .to_string()
}
