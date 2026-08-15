//! Wire-size and advertised-name guard for the `tools/list` surface.
//!
//! Drives the built `assay-mcp-server` over a real handshake and records compact
//! JSON-RPC bytes. Live witness is structural (spawn `CARGO_BIN_EXE_assay-mcp-server`,
//! initialize, then `tools/list`); a fixture cannot pass by flipping a boolean.
//! Cardinality is checked before BTreeSet comparison so a duplicate advertised
//! name fails. Default-feature vs `--features test-outbound` expectations are
//! explicit. The only scored size bound is a 16 KiB explosion ceiling on the
//! complete compact response; 8,120 is diagnostic, not a lock. Name-set
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
const EXPECTED_LEN: usize = if cfg!(feature = "test-outbound") {
    6
} else {
    5
};

fn expected_names() -> Vec<String> {
    let mut names: Vec<String> = PRODUCTION_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    if cfg!(feature = "test-outbound") {
        names.push(TEST_OUTBOUND_TOOL.to_string());
    }
    names
}

fn assert_live_tool_cardinality(names: &[String]) {
    assert_eq!(
        names.len(),
        EXPECTED_LEN,
        "tools/list live response must have exactly {EXPECTED_LEN} entries before name-set comparison (got {} names: {names:?})",
        names.len()
    );
}

fn assert_advertised_tool_names(names: &[String]) {
    assert_live_tool_cardinality(names);
    let actual: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let expected_owned = expected_names();
    let expected: BTreeSet<&str> = expected_owned.iter().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "tools/list advertised names must match the expected set (got {actual:?})"
    );
}

fn assert_test_outbound_expectation(names: &[String]) {
    let advertised = names.iter().any(|name| name == TEST_OUTBOUND_TOOL);
    if cfg!(feature = "test-outbound") {
        assert!(
            advertised,
            "test-outbound tools/list must advertise {TEST_OUTBOUND_TOOL}"
        );
    } else {
        assert!(
            !advertised,
            "default-feature tools/list advertised {TEST_OUTBOUND_TOOL}"
        );
    }
}

fn assert_compact_response_within_ceiling(compact: &[u8], ceiling: usize) {
    assert!(
        compact.len() <= ceiling,
        "compact tools/list JSON-RPC response is {} bytes, exceeds the {ceiling}-byte explosion ceiling",
        compact.len()
    );
}

fn assert_live_handshake(initialize: &Value, list_response: &Value) {
    assert!(
        initialize.get("result").is_some(),
        "live handshake via CARGO_BIN_EXE_assay-mcp-server: initialize must have result, got {initialize:?}"
    );
    assert!(
        list_response
            .get("result")
            .and_then(|result| result.get("tools"))
            .and_then(Value::as_array)
            .is_some(),
        "live handshake via CARGO_BIN_EXE_assay-mcp-server: tools/list must have result.tools array, got {list_response:?}"
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

fn spawn_server(policy_root: &Path) -> Conn {
    let child = clean_server_command()
        .arg("--policy-root")
        .arg(policy_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn CARGO_BIN_EXE_assay-mcp-server");
    Conn::attach(child)
}

fn observe_live_tools_list() -> (Vec<String>, Vec<u8>, Value) {
    let policy_root = tempfile::tempdir().expect("temp policy-root");
    let mut conn = spawn_server(policy_root.path());

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

    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }));
    let list_response = conn.read_response_for_id(2);
    let _ = conn.shutdown();

    assert_live_handshake(&initialize, &list_response);
    let names = tool_names(&list_response);
    let compact =
        serde_json::to_vec(&list_response).expect("serialize complete tools/list response");
    (names, compact, list_response)
}

#[test]
fn live_tools_list_matches_feature_mode_under_16kib() {
    let (names, compact, response) = observe_live_tools_list();
    assert_advertised_tool_names(&names);
    assert_test_outbound_expectation(&names);
    eprint_wire_diagnostics(&response, &compact);
    assert_compact_response_within_ceiling(&compact, COMPACT_RESPONSE_CEILING);
}

#[test]
#[should_panic(expected = "entries before name-set comparison")]
fn mutation_duplicate_production_tool_fails_cardinality_guard() {
    let mut names = expected_names();
    names.push(names[0].clone());
    assert_advertised_tool_names(&names);
}

#[test]
#[should_panic(expected = "CARGO_BIN_EXE_assay-mcp-server")]
fn mutation_fixture_initialize_fails_live_handshake_guard() {
    assert_live_handshake(
        &serde_json::json!({}),
        &serde_json::json!({"result": {"tools": []}}),
    );
}

#[cfg(not(feature = "test-outbound"))]
#[test]
#[should_panic(expected = "default-feature tools/list advertised assay_test_outbound")]
fn mutation_injected_assay_test_outbound_fails_outbound_guard() {
    let mut names = expected_names();
    names.push(TEST_OUTBOUND_TOOL.to_string());
    assert_test_outbound_expectation(&names);
}

#[cfg(feature = "test-outbound")]
#[test]
#[should_panic(expected = "test-outbound tools/list must advertise assay_test_outbound")]
fn mutation_dropped_assay_test_outbound_fails_outbound_guard() {
    let mut names = expected_names();
    names.retain(|name| name != TEST_OUTBOUND_TOOL);
    assert_test_outbound_expectation(&names);
}

#[test]
#[should_panic(expected = "tools/list advertised names must match the expected set")]
fn mutation_renamed_production_tool_fails_names_guard() {
    let mut names = expected_names();
    names[0] = "assay_check_args_renamed".to_string();
    assert_advertised_tool_names(&names);
}

#[test]
#[should_panic(expected = "entries before name-set comparison")]
fn mutation_removed_production_tool_fails_names_guard() {
    let mut names = expected_names();
    names.remove(0);
    assert_advertised_tool_names(&names);
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
