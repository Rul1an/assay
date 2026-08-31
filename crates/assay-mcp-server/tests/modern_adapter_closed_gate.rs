//! Closed-gate control for the modern stateless adapter (#2482).
//!
//! The adapter can serve. The public stdio loop must not. These tests pair the
//! same request shapes against both surfaces and assert over shipping constants.

#![allow(deprecated)]

use assay_mcp_server::modern_adapter::{
    serve, CachePolicy, StatelessAdapter, CACHE_SCOPE, CACHE_TTL_MS, ERROR_INVALID_PARAMS,
};
use assay_mcp_server::server::{
    ACCEPTED_PROTOCOL_VERSIONS, ERROR_METHOD_NOT_FOUND, ERROR_UNSUPPORTED_PROTOCOL_VERSION,
    MODERN_PROTOCOL_VERSION,
};
use assay_mcp_server::tools::{list_tools, ToolContext};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const SERVER_SRC: &str = include_str!("../src/server.rs");
const LATEST_LEGACY_VERSION: &str = ACCEPTED_PROTOCOL_VERSIONS[2];
const ACTIVATION_GUIDANCE: &str =
    "#2483 requires release-bound activation evidence; preserve historical release observations";

fn modern_params() -> Value {
    json!({
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    })
}

fn modern_request(id: u64, method: &str, extra: Value) -> Value {
    let mut params = modern_params();
    if let Some(object) = extra.as_object() {
        for (key, value) in object {
            params[key] = value.clone();
        }
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    })
}

fn policy_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mcp")
}

fn tool_context() -> ToolContext {
    let root = policy_root();
    let canon = std::fs::canonicalize(&root).expect("policy root");
    ToolContext {
        policy_root: root,
        policy_root_canon: canon,
        cfg: assay_mcp_server::config::ServerConfig::default(),
        caches: assay_mcp_server::cache::PolicyCaches::new(8),
    }
}

fn run_session(requests: &[Value]) -> Output {
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
    let mut child = command
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

fn live_response(request: &Value) -> Value {
    let output = run_session(std::slice::from_ref(request));
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .expect("one response"),
    )
    .expect("JSON-RPC response")
}

fn assert_cacheable(result: &Value) {
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["ttlMs"], CACHE_TTL_MS);
    assert_eq!(result["cacheScope"], CACHE_SCOPE);
    assert_eq!(CachePolicy::INITIAL.ttl_ms, CACHE_TTL_MS);
    assert_eq!(CachePolicy::INITIAL.scope, CACHE_SCOPE);
}

#[test]
fn shipping_accepted_set_excludes_the_modern_revision() {
    closed_gate(ACCEPTED_PROTOCOL_VERSIONS, &public_capabilities()).unwrap();
    assert_eq!(ERROR_UNSUPPORTED_PROTOCOL_VERSION, -32022);
    assert_eq!(ERROR_METHOD_NOT_FOUND, -32601);
}

fn public_capabilities() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/data/product-capabilities.v0.json");
    let source = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("checked-in public capability data: {error}; {ACTIVATION_GUIDANCE}")
    });
    serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("public capability JSON: {error}; {ACTIVATION_GUIDANCE}"))
}

// Historical release evidence is a closed-era ceiling, not a mirror of future HEAD support.
fn closed_gate(accepted: &[&str], capabilities: &Value) -> Result<(), String> {
    let fail = |reason: &str| format!("{reason}; {ACTIVATION_GUIDANCE}");
    if accepted.is_empty() || accepted.contains(&MODERN_PROTOCOL_VERSION) {
        return Err(fail(
            "production accepted-version set must be nonempty and exclude modern",
        ));
    }
    let rows = capabilities
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("capabilities must be an array"))?;
    for id in ["published-mcp-server", "published-release-golden-path"] {
        let mut matches = rows.iter().filter(|row| row["id"] == id);
        let row = matches
            .next()
            .ok_or_else(|| fail(&format!("missing {id}")))?;
        if matches.next().is_some() {
            return Err(fail(&format!("duplicate {id}")));
        }
        let versions = row
            .get("protocol_versions")
            .and_then(Value::as_array)
            .filter(|versions| !versions.is_empty())
            .ok_or_else(|| fail(&format!("{id}: protocol_versions must be a nonempty array")))?;
        for version in versions {
            for field in ["protocol", "version", "transport"] {
                if version
                    .get(field)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .is_none()
                {
                    return Err(fail(&format!("{id}: {field} must be a nonempty string")));
                }
            }
            if version["version"] == MODERN_PROTOCOL_VERSION {
                return Err(fail(&format!("{id}: public support must exclude modern")));
            }
        }
    }
    Ok(())
}

fn assert_closed_gate_rejects(accepted: &[&str], capabilities: &Value, reason: &str) {
    let error = closed_gate(accepted, capabilities).expect_err("invalid closed-era claim accepted");
    assert!(error.contains(reason), "{error}");
    assert!(error.contains("#2483"), "{error}");
    assert!(
        error.contains("release-bound activation evidence"),
        "{error}"
    );
}

#[test]
fn closed_gate_rejects_production_only_activation_with_owner_diagnostic() {
    let mut accepted = ACCEPTED_PROTOCOL_VERSIONS.to_vec();
    accepted.push(MODERN_PROTOCOL_VERSION);
    assert_closed_gate_rejects(&accepted, &public_capabilities(), "production");
    assert_closed_gate_rejects(&[], &public_capabilities(), "production");
}

#[test]
fn closed_gate_rejects_public_only_activation_in_either_row() {
    for id in ["published-mcp-server", "published-release-golden-path"] {
        let mut capabilities = public_capabilities();
        let row = capabilities["capabilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["id"] == id)
            .unwrap();
        row["protocol_versions"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "protocol": "mcp", "version": MODERN_PROTOCOL_VERSION, "transport": "stdio"
            }));
        assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &capabilities, id);
    }
}

#[test]
fn closed_gate_requires_each_public_row_and_typed_version_collection() {
    for id in ["published-mcp-server", "published-release-golden-path"] {
        let original = public_capabilities();
        let mut missing = original.clone();
        missing["capabilities"]
            .as_array_mut()
            .unwrap()
            .retain(|row| row["id"] != id);
        assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &missing, id);

        let mut duplicate = original.clone();
        let row = original["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap()
            .clone();
        duplicate["capabilities"].as_array_mut().unwrap().push(row);
        assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &duplicate, id);

        let mut absent_versions = original.clone();
        let row = absent_versions["capabilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["id"] == id)
            .unwrap();
        row.as_object_mut().unwrap().remove("protocol_versions");
        assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &absent_versions, id);

        for versions in [
            Value::Null,
            json!({}),
            json!([]),
            json!([{}]),
            json!(["2025-11-25"]),
            json!([{"protocol":"mcp","version":42,"transport":"stdio"}]),
            json!([{"protocol":"mcp","version":"2025-11-25"}]),
        ] {
            let mut malformed = original.clone();
            let row = malformed["capabilities"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|row| row["id"] == id)
                .unwrap();
            row["protocol_versions"] = versions;
            assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &malformed, id);
        }
        for field in ["protocol", "version", "transport"] {
            for value in [Value::Null, json!(42), json!(""), json!([])] {
                let mut malformed = original.clone();
                let row = malformed["capabilities"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| row["id"] == id)
                    .unwrap();
                row["protocol_versions"][0][field] = value;
                assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &malformed, id);
            }
        }
    }
    for capabilities in [
        json!({}),
        json!({"capabilities":null}),
        json!({"capabilities":{}}),
    ] {
        assert_closed_gate_rejects(ACCEPTED_PROTOCOL_VERSIONS, &capabilities, "capabilities");
    }
}

#[test]
fn public_dispatch_source_does_not_name_the_adapter() {
    assert!(
        !SERVER_SRC.contains("modern_adapter"),
        "Server::run named the modern adapter; that is a public dispatch path"
    );
}

#[tokio::test]
async fn adapter_serves_discover_and_deterministic_tools_list() {
    let discover = modern_request(1, "server/discover", json!({}));
    let listed = modern_request(2, "tools/list", json!({}));

    let discover_response = serve(&discover, None).await;
    let list_response = serve(&listed, None).await;

    assert!(
        discover_response.get("error").is_none(),
        "{discover_response}"
    );
    let discover_result = &discover_response["result"];
    assert_cacheable(discover_result);
    assert!(discover_result["capabilities"].is_object());
    assert_eq!(
        discover_result["supportedVersions"],
        json!([MODERN_PROTOCOL_VERSION])
    );

    assert!(list_response.get("error").is_none(), "{list_response}");
    let list_result = &list_response["result"];
    assert_cacheable(list_result);
    let names: Vec<&str> = list_result["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    let shipped_tools = list_tools();
    let shipped: Vec<&str> = shipped_tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, shipped);
}

#[tokio::test]
async fn adapter_drives_all_five_tools_and_unknown_tool() {
    let ctx = tool_context();
    let calls = [
        (
            "assay_check_args",
            json!({"tool": "discount_tool", "arguments": {"percent": 10}, "policy": "policy.yaml"}),
        ),
        (
            "assay_check_sequence",
            json!({"history": [], "next_tool": "action", "policy": "sequence_policy.yaml"}),
        ),
        (
            "assay_policy_decide",
            json!({"tool": "dangerous_tool_suffix", "policy": "blocklist_policy.yaml"}),
        ),
        (
            "assay_check_coverage",
            json!({"policy": "policy.yaml", "traces": [{"tools": ["discount_tool"]}]}),
        ),
        (
            "assay_explain_trace",
            json!({"policy": "policy.yaml", "trace": [{"tool": "discount_tool"}]}),
        ),
    ];

    for (index, (name, arguments)) in calls.into_iter().enumerate() {
        let request = modern_request(
            (index + 1) as u64,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
        let response = serve(&request, Some(&ctx)).await;
        assert!(
            response.get("error").is_none(),
            "{name} protocol error: {response}"
        );
        assert_eq!(response["result"]["resultType"], "complete");
        assert!(response["result"]["content"].is_array(), "{name}");
    }

    let unknown = modern_request(
        99,
        "tools/call",
        json!({"name": "not_a_release_tool", "arguments": {}}),
    );
    let response = serve(&unknown, Some(&ctx)).await;
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(response["result"]["resultType"], "complete");
}

#[tokio::test]
async fn adapter_refuses_missing_metadata_and_unknown_methods() {
    let mut missing_version = modern_request(1, "tools/list", json!({}));
    missing_version["params"]["_meta"]
        .as_object_mut()
        .expect("meta")
        .remove("io.modelcontextprotocol/protocolVersion");
    let refused = serve(&missing_version, None).await;
    assert_eq!(refused["error"]["code"], ERROR_INVALID_PARAMS);

    let unknown = modern_request(2, "prompts/list", json!({}));
    let missing = serve(&unknown, None).await;
    assert_eq!(missing["error"]["code"], ERROR_METHOD_NOT_FOUND);
}

#[tokio::test]
async fn two_adapter_instances_serve_interleaved_requests() {
    let left = StatelessAdapter::new();
    let right = StatelessAdapter::new();
    let first = modern_request(1, "server/discover", json!({}));
    let second = modern_request(2, "tools/list", json!({}));

    let from_right = right.serve(&first, None).await;
    let from_left = left.serve(&second, None).await;

    assert!(from_right["result"]["supportedVersions"].is_array());
    assert!(from_left["result"]["tools"].is_array());
}

#[tokio::test]
async fn the_same_adapter_requests_are_refused_on_the_public_wire() {
    let discover = modern_request(1, "server/discover", json!({}));
    let listed = modern_request(2, "tools/list", json!({}));
    let call = modern_request(
        3,
        "tools/call",
        json!({
            "name": "assay_check_args",
            "arguments": {
                "tool": "discount_tool",
                "arguments": {"percent": 10},
                "policy": "policy.yaml"
            }
        }),
    );
    let empty_discover = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "server/discover",
        "params": {}
    });

    assert!(serve(&discover, None).await.get("result").is_some());
    assert!(serve(&listed, None).await.get("result").is_some());

    let live_discover = live_response(&discover);
    assert_eq!(
        live_discover["error"]["code"],
        ERROR_UNSUPPORTED_PROTOCOL_VERSION
    );
    assert_eq!(
        live_discover["error"]["data"]["requested"],
        MODERN_PROTOCOL_VERSION
    );
    assert_eq!(
        live_discover["error"]["data"]["supported"],
        json!(ACCEPTED_PROTOCOL_VERSIONS)
    );
    assert!(live_discover.get("result").is_none());

    let live_list = live_response(&listed);
    assert_eq!(
        live_list["error"]["code"],
        ERROR_UNSUPPORTED_PROTOCOL_VERSION
    );
    assert_eq!(
        live_list["error"]["data"]["supported"],
        json!(ACCEPTED_PROTOCOL_VERSIONS)
    );
    assert!(live_list.get("result").is_none());

    let live_call = live_response(&call);
    assert_eq!(
        live_call["error"]["code"],
        ERROR_UNSUPPORTED_PROTOCOL_VERSION
    );
    assert!(live_call.get("result").is_none());

    let live_empty = live_response(&empty_discover);
    assert_eq!(live_empty["error"]["code"], ERROR_METHOD_NOT_FOUND);
    assert!(live_empty.get("result").is_none());
}

#[test]
fn live_legacy_path_still_drives_all_five_release_tools() {
    let mut requests = vec![json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": LATEST_LEGACY_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "contract-test", "version": "1.0"}
        }
    })];
    let calls = [
        (
            "assay_check_args",
            json!({"tool": "discount_tool", "arguments": {"percent": 10}, "policy": "policy.yaml"}),
        ),
        (
            "assay_check_sequence",
            json!({"history": [], "next_tool": "action", "policy": "sequence_policy.yaml"}),
        ),
        (
            "assay_policy_decide",
            json!({"tool": "dangerous_tool_suffix", "policy": "blocklist_policy.yaml"}),
        ),
        (
            "assay_check_coverage",
            json!({"policy": "policy.yaml", "traces": [{"tools": ["discount_tool"]}]}),
        ),
        (
            "assay_explain_trace",
            json!({"policy": "policy.yaml", "trace": [{"tool": "discount_tool"}]}),
        ),
    ];
    for (offset, (name, arguments)) in calls.into_iter().enumerate() {
        requests.push(json!({
            "jsonrpc": "2.0",
            "id": offset + 2,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
    }

    let output = run_session(&requests);
    let parsed: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(parsed.len(), 6);
    assert_eq!(
        parsed[0]["result"]["protocolVersion"],
        LATEST_LEGACY_VERSION
    );
    for response in &parsed[1..] {
        assert!(
            response.get("result").is_some(),
            "legacy five-tool path lost a result: {response}"
        );
        assert!(response.get("error").is_none(), "{response}");
    }
}

#[test]
fn legacy_initialize_fallback_is_unchanged() {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": MODERN_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "contract-test", "version": "1.0"}
        }
    });
    let response = live_response(&request);
    assert_eq!(response["result"]["protocolVersion"], LATEST_LEGACY_VERSION);
    assert!(response.get("error").is_none());
}
