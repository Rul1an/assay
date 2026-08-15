//! Wire-size guard for the default-feature `tools/list` surface.
//!
//! Drives the built `assay-mcp-server` over a real handshake and records compact
//! JSON-RPC bytes. The only scored size bound is a 16 KiB explosion ceiling on
//! the complete compact response; 8,120 is diagnostic, not a lock. Name-set
//! ownership for install-surface launch stays in `project_install_surfaces.rs`.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const PRODUCTION_TOOL_NAMES: [&str; 5] = [
    "assay_check_args",
    "assay_check_sequence",
    "assay_policy_decide",
    "assay_check_coverage",
    "assay_explain_trace",
];

const TEST_OUTBOUND_TOOL: &str = "assay_test_outbound";
const COMPACT_RESPONSE_CEILING: usize = 16_384;

/// Compact `tools/list` observation. The live path tags `from_built_binary: true`.
struct ToolsListObservation {
    names: Vec<String>,
    compact: Vec<u8>,
    from_built_binary: bool,
}

fn assert_production_tool_names(names: &[String]) {
    let actual: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let expected: BTreeSet<&str> = PRODUCTION_TOOL_NAMES.iter().copied().collect();
    assert_eq!(
        actual, expected,
        "tools/list production names must be exactly the five advertised tools (got {actual:?})"
    );
}

fn assert_test_outbound_absent(names: &[String]) {
    assert!(
        !names.iter().any(|name| name == TEST_OUTBOUND_TOOL),
        "default-feature tools/list advertised {TEST_OUTBOUND_TOOL}"
    );
}

fn assert_compact_response_within_ceiling(compact: &[u8], ceiling: usize) {
    assert!(
        compact.len() <= ceiling,
        "compact tools/list JSON-RPC response is {} bytes, exceeds the {ceiling}-byte explosion ceiling",
        compact.len()
    );
}

fn assert_from_built_binary(observation: &ToolsListObservation) {
    assert!(
        observation.from_built_binary,
        "tools/list observation must come from the built assay-mcp-server binary (from_built_binary: {})",
        observation.from_built_binary
    );
}

fn tool_names(response: &Value) -> Vec<String> {
    response["result"]["tools"]
        .as_array()
        .expect("tools/list result.tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name").to_string())
        .collect()
}

fn eprint_wire_diagnostics(response: &Value, compact: &[u8]) {
    let result_bytes = serde_json::to_vec(&response["result"]).expect("serialize result");
    eprintln!(
        "tools/list compact JSON-RPC response: {} bytes (ceiling {COMPACT_RESPONSE_CEILING})",
        compact.len()
    );
    eprintln!("tools/list compact result: {} bytes", result_bytes.len());
    let Some(tools) = response["result"]["tools"].as_array() else {
        return;
    };
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("<unnamed>");
        let whole = serde_json::to_vec(tool).expect("serialize tool").len();
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .map(str::len)
            .unwrap_or(0);
        let schema = tool
            .get("inputSchema")
            .map(|schema| {
                serde_json::to_vec(schema)
                    .expect("serialize inputSchema")
                    .len()
            })
            .unwrap_or(0);
        eprintln!("  {name}: whole={whole} description={description} inputSchema={schema}");
    }
}

fn clean_server_command() -> Command {
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

fn spawn_default_server(policy_root: &Path) -> Conn {
    let child = clean_server_command()
        .arg("--policy-root")
        .arg(policy_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn default-feature assay-mcp-server");
    Conn::attach(child)
}

fn production_names() -> Vec<String> {
    PRODUCTION_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

fn observe_live_tools_list() -> (Value, ToolsListObservation) {
    let policy_root = tempfile::tempdir().expect("temp policy-root");
    let mut conn = spawn_default_server(policy_root.path());

    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "tools-list-wire", "version": "1.0"}
        }
    }));
    let initialize = conn.read_response_for_id(1);
    assert!(
        initialize.get("result").is_some(),
        "initialize failed: {initialize:?}"
    );

    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }));
    let response = conn.read_response_for_id(2);
    let _ = conn.shutdown();

    let names = tool_names(&response);
    let compact = serde_json::to_vec(&response).expect("serialize complete tools/list response");
    let observation = ToolsListObservation {
        names,
        compact,
        from_built_binary: true,
    };
    (response, observation)
}

fn hardcoded_fixture_observation() -> ToolsListObservation {
    ToolsListObservation {
        names: production_names(),
        compact: vec![0x7b; 8_120],
        from_built_binary: false,
    }
}

#[test]
fn default_feature_tools_list_wire_is_five_production_tools_under_16kib() {
    let (response, observation) = observe_live_tools_list();
    assert_from_built_binary(&observation);
    assert_production_tool_names(&observation.names);
    assert_test_outbound_absent(&observation.names);

    eprint_wire_diagnostics(&response, &observation.compact);
    assert_compact_response_within_ceiling(&observation.compact, COMPACT_RESPONSE_CEILING);
}

#[test]
#[should_panic(expected = "tools/list production names must be exactly the five advertised tools")]
fn mutation_renamed_production_tool_fails_names_guard() {
    let mut names = production_names();
    names[0] = "assay_check_args_renamed".to_string();
    assert_production_tool_names(&names);
}

#[test]
#[should_panic(expected = "tools/list production names must be exactly the five advertised tools")]
fn mutation_removed_production_tool_fails_names_guard() {
    let mut names = production_names();
    names.remove(0);
    assert_production_tool_names(&names);
}

#[test]
#[should_panic(expected = "default-feature tools/list advertised assay_test_outbound")]
fn mutation_injected_assay_test_outbound_fails_outbound_guard() {
    let mut names = production_names();
    names.push(TEST_OUTBOUND_TOOL.to_string());
    assert_test_outbound_absent(&names);
}

#[test]
#[should_panic(expected = "exceeds the 8119-byte explosion ceiling")]
fn mutation_lowered_ceiling_below_live_compact_body_fails_ceiling_guard() {
    // Mutation fixture only: a body the length of today's compact response, with
    // the ceiling lowered below that body. The live test still uses 16_384.
    let live_compact_copy = vec![0x7b; 8_120];
    let lowered_ceiling = live_compact_copy.len() - 1;
    assert_compact_response_within_ceiling(&live_compact_copy, lowered_ceiling);
}

#[test]
#[should_panic(expected = "from_built_binary: false")]
fn mutation_hardcoded_fixture_fails_built_binary_guard() {
    let fixture = hardcoded_fixture_observation();
    assert_production_tool_names(&fixture.names);
    assert_test_outbound_absent(&fixture.names);
    assert_compact_response_within_ceiling(&fixture.compact, COMPACT_RESPONSE_CEILING);
    assert_from_built_binary(&fixture);
}
