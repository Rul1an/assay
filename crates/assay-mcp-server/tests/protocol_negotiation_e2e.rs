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
            Some("Invalid initialize params: expected the required legacy MCP fields")
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
fn incomplete_initialize_shape_is_value_free_invalid_params() {
    let secret = "malformed-initialize-value-must-not-be-echoed";
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": LATEST_LEGACY_VERSION,
                "clientInfo": {"name": "contract-test", "version": "1.0"}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "initialize",
            "params": {
                "protocolVersion": LATEST_LEGACY_VERSION,
                "capabilities": [],
                "clientInfo": {"name": "contract-test", "version": "1.0"}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "initialize",
            "params": {
                "protocolVersion": LATEST_LEGACY_VERSION,
                "capabilities": {},
                "clientInfo": {"name": secret}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "initialize",
            "params": {
                "protocolVersion": LATEST_LEGACY_VERSION,
                "capabilities": {},
                "clientInfo": secret
            }
        }),
    ];

    let output = run_session(&requests);
    let parsed = responses(&output);
    assert_eq!(parsed.len(), requests.len());
    for response in parsed {
        assert_eq!(response["error"]["code"].as_i64(), Some(-32602));
        assert_eq!(
            response["error"]["message"].as_str(),
            Some("Invalid initialize params: expected the required legacy MCP fields")
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

/// The probe a dual-era client sends first is now answered rather than refused.
///
/// This replaces `modern_discovery_probe_remains_a_method_not_found_fallback_signal`, which pinned
/// the legacy-only behaviour: `server/discover` returned `-32601`, which a dual-era client
/// recognises as a non-modern signal and falls back from. That fallback was the whole compatibility
/// story, and it left modern-only clients with no path at all (#1943).
///
/// What the old test protected -- a dual-era client can use this proxy -- still holds. It now holds
/// by discovery instead of by fallback, which is the stronger of the two.
#[test]
fn the_discovery_probe_is_answered() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {}
    });
    let output = run_session(&[request]);
    let response = responses(&output).pop().expect("discover response");
    assert!(response.get("error").is_none(), "{response}");
    let versions = response["result"]["protocolVersions"]
        .as_array()
        .expect("protocolVersions");
    assert!(versions.iter().any(|v| v == "2026-07-28"));
    assert!(versions.iter().any(|v| v == LATEST_LEGACY_VERSION));
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert_eq!(response["result"]["serverInfo"]["name"], "assay-mcp-server");
}

/// A modern request carries its revision and needs no handshake.
///
/// This is what a modern-only client does, and what this proxy could not serve at all before: the
/// spec's compatibility matrix marks Modern x Legacy as failing with no fall-forward.
#[test]
fn a_modern_request_is_served_without_a_handshake() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } }
    });
    let output = run_session(&[request]);
    let response = responses(&output).pop().expect("tools/list response");
    assert!(response.get("error").is_none(), "{response}");
    assert!(response["result"]["tools"].is_array());
}

/// A revision this server does not implement is refused by name, with the set it does implement.
///
/// Not served on a best-effort basis, which is the silent behaviour the 2026-07-28 spec replaced,
/// and not a bare error either -- the `data` is what lets a client fall forward without a second
/// round trip.
#[test]
fn an_unimplemented_revision_is_refused_with_the_supported_set() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2099-01-01" } }
    });
    let output = run_session(&[request]);
    let response = responses(&output).pop().expect("response");
    assert_eq!(response["error"]["code"].as_i64(), Some(-32022));
    assert_eq!(response["error"]["data"]["requested"], "2099-01-01");
    let supported = response["error"]["data"]["supported"]
        .as_array()
        .expect("supported set");
    assert!(supported.iter().any(|v| v == "2026-07-28"));
    assert!(response.get("result").is_none());
}

/// What `server/discover` advertises is what the version check accepts.
///
/// Two lists would let a client discover a revision it is then refused, which is worse than not
/// advertising it -- so this drives both through the wire rather than reading the constant.
#[test]
fn every_advertised_revision_is_accepted() {
    let discover = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}
    });
    let output = run_session(&[discover]);
    let response = responses(&output).pop().expect("discover response");
    let versions: Vec<String> = response["result"]["protocolVersions"]
        .as_array()
        .expect("protocolVersions")
        .iter()
        .map(|v| v.as_str().expect("version string").to_string())
        .collect();
    assert!(!versions.is_empty());

    for version in versions {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": version } }
        });
        let output = run_session(&[request]);
        let response = responses(&output).pop().expect("response");
        assert!(
            response.get("error").is_none(),
            "discover advertises {version} and the version check refuses it: {response}"
        );
    }
}

/// The legacy handshake is untouched. A dual-era server serves both, and this is the half that
/// was already working -- pinned so making the modern path work cannot quietly break it.
#[test]
fn the_legacy_handshake_still_works_alongside_discovery() {
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}
        }),
        initialize_request(Value::String(LATEST_LEGACY_VERSION.to_string()), 2),
    ];
    let output = run_session(&requests);
    let responses = responses(&output);
    assert!(responses[0].get("error").is_none(), "{}", responses[0]);
    assert_eq!(
        responses[1]["result"]["protocolVersion"],
        LATEST_LEGACY_VERSION
    );
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
