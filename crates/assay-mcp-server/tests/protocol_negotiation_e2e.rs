use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const LATEST_LEGACY_VERSION: &str = "2025-11-25";

fn policy_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mcp")
}

fn clean_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_AUTH_")
        {
            command.env_remove(name);
        }
    }
    command
}

fn run_session(requests: &[Value]) -> Output {
    let mut child = clean_command()
        .arg("--policy-root")
        .arg(policy_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assay-mcp-server");

    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for request in requests {
            writeln!(stdin, "{request}").expect("write JSON-RPC request");
        }
    }

    child.wait_with_output().expect("wait for server")
}

fn responses(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect()
}

fn initialize_request(protocol_version: Value, id: u64) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {"name": "contract-test", "version": "1.0"}
        }
    })
}

#[test]
fn supported_legacy_versions_are_echoed_exactly() {
    for version in ["2024-11-05", LATEST_LEGACY_VERSION] {
        let output = run_session(&[initialize_request(Value::String(version.to_string()), 1)]);
        let response = responses(&output).pop().expect("initialize response");
        assert_eq!(
            response["result"]["protocolVersion"].as_str(),
            Some(version),
            "supported legacy version must be echoed"
        );
    }
}

#[test]
fn unsupported_versions_negotiate_to_the_latest_legacy_revision() {
    for requested in ["2025-06-18", "2026-07-28", "future-version"] {
        let output = run_session(&[initialize_request(Value::String(requested.to_string()), 1)]);
        let response = responses(&output).pop().expect("initialize response");
        assert_eq!(
            response["result"]["protocolVersion"].as_str(),
            Some(LATEST_LEGACY_VERSION),
            "unsupported input must not become an advertised protocol version"
        );
    }
}

#[test]
fn missing_or_non_string_protocol_version_is_value_free_invalid_params() {
    let secret = "protocol-version-value-must-not-be-echoed";
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {},
                "clientInfo": {"name": "contract-test", "version": "1.0"}
            }
        }),
        initialize_request(serde_json::json!({"secret": secret}), 2),
    ];

    let output = run_session(&requests);
    let parsed = responses(&output);
    assert_eq!(parsed.len(), 2);
    for response in parsed {
        assert_eq!(response["error"]["code"].as_i64(), Some(-32602));
        assert_eq!(
            response["error"]["message"].as_str(),
            Some("Invalid initialize params: protocolVersion must be a string")
        );
        assert!(response.get("result").is_none());
    }

    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !rendered.contains(secret),
        "invalid input value leaked into protocol output or logs"
    );
}

#[test]
fn modern_discovery_probe_remains_a_method_not_found_fallback_signal() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {}
    });
    let output = run_session(&[request]);
    let response = responses(&output).pop().expect("discover response");
    assert_eq!(response["error"]["code"].as_i64(), Some(-32601));
    assert!(response.get("result").is_none());
}

#[test]
fn latest_legacy_handshake_preserves_tools_list_and_call() {
    let requests = [
        initialize_request(Value::String(LATEST_LEGACY_VERSION.to_string()), 1),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "assay_check_args",
                "arguments": {
                    "tool": "discount_tool",
                    "arguments": {"percent": 10},
                    "policy": "policy.yaml"
                }
            }
        }),
    ];

    let output = run_session(&requests);
    let parsed = responses(&output);
    assert_eq!(parsed.len(), 3);
    assert_eq!(
        parsed[0]["result"]["protocolVersion"].as_str(),
        Some(LATEST_LEGACY_VERSION)
    );
    assert!(parsed[1]["result"]["tools"].is_array());
    assert_eq!(parsed[2]["result"]["isError"].as_bool(), Some(false));
}
