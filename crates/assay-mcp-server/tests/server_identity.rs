//! Live stdio contract for `assay_server_identity`.
//!
//! The tool lets a client read the same self-reported `serverInfo` that `initialize` returns, from
//! inside a session that only exposes tool results. It is a self-report of the running build's
//! compile-time name and version: not an attestation of the binary, its path, or where the server's
//! definition came from.
//!
//! Driven over a real handshake against `CARGO_BIN_EXE_assay-mcp-server`, so `tools/list`
//! advertisement, `tools/call` dispatch and the CallToolResult wrapping are the shipped ones.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const TOOL: &str = "assay_server_identity";

fn spawn_server(policy_root: &Path) -> Conn {
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
    let child = command
        .arg("--policy-root")
        .arg(policy_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn CARGO_BIN_EXE_assay-mcp-server");
    Conn::attach(child)
}

/// Initialize, then return the connection and the `serverInfo` the handshake reported.
fn initialized(policy_root: &Path) -> (Conn, Value) {
    let mut conn = spawn_server(policy_root);
    conn.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "server-identity", "version": "1.0"}
        }
    }));
    let initialize = conn.read_response_for_id(1);
    let server_info = initialize["result"]["serverInfo"].clone();
    assert!(
        server_info.is_object(),
        "initialize must report serverInfo: {initialize}"
    );
    conn.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    (conn, server_info)
}

fn call(conn: &mut Conn, id: u64, params: Value) -> Value {
    conn.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params}));
    conn.read_response_for_id(id)
}

/// The one text content item of a CallToolResult, parsed as JSON.
fn text_payload(response: &Value) -> Value {
    let content = response["result"]["content"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/call must return result.content: {response}"));
    assert_eq!(content.len(), 1, "exactly one content item: {response}");
    assert_eq!(content[0]["type"], "text", "text content: {response}");
    serde_json::from_str(content[0]["text"].as_str().expect("text string"))
        .expect("text content is JSON")
}

#[test]
fn identity_tool_is_advertised_read_only_with_an_exactly_empty_argument_contract() {
    let root = tempfile::tempdir().expect("temp policy-root");
    let (mut conn, _) = initialized(root.path());
    conn.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let list = conn.read_response_for_id(2);
    let _ = conn.shutdown();

    let tools = list["result"]["tools"].as_array().expect("tools array");
    let matching: Vec<&Value> = tools.iter().filter(|t| t["name"] == TOOL).collect();
    assert_eq!(matching.len(), 1, "{TOOL} advertised exactly once: {list}");
    let tool = matching[0];
    assert_eq!(
        tool["inputSchema"],
        json!({"type": "object", "properties": {}, "additionalProperties": false}),
        "argument contract must be exactly the empty object"
    );
    assert_eq!(
        tool["annotations"],
        json!({"readOnlyHint": true}),
        "read-only annotation"
    );
    let description = tool["description"].as_str().expect("description");
    assert!(
        description.contains("self-report"),
        "description must say this is a self-report: {description}"
    );
}

#[test]
fn identity_call_returns_exactly_the_initialize_server_info() {
    let root = tempfile::tempdir().expect("temp policy-root");
    let (mut conn, server_info) = initialized(root.path());
    let with_empty = call(&mut conn, 3, json!({"name": TOOL, "arguments": {}}));
    let with_omitted = call(&mut conn, 4, json!({"name": TOOL}));
    let _ = conn.shutdown();

    // The literal expectation, independent of the server, so parity cannot pass by both sides
    // drifting together.
    let expected = json!({"name": "assay-mcp-server", "version": env!("CARGO_PKG_VERSION")});
    assert_eq!(server_info, expected, "initialize serverInfo");
    for response in [&with_empty, &with_omitted] {
        assert_eq!(
            response["result"]["isError"],
            json!(false),
            "identity is not an error: {response}"
        );
        assert!(
            response["result"].get("structuredContent").is_none(),
            "no structuredContent on success: {response}"
        );
        let payload = text_payload(response);
        assert_eq!(
            payload, server_info,
            "tool result must equal initialize serverInfo"
        );
        assert_eq!(payload, expected, "tool result must equal the literal");
    }
}

#[test]
fn identity_call_with_any_argument_is_refused_without_reflecting_it() {
    let root = tempfile::tempdir().expect("temp policy-root");
    let (mut conn, _) = initialized(root.path());
    let response = call(
        &mut conn,
        5,
        json!({"name": TOOL, "arguments": {"probeKEY": "probeVALUE"}}),
    );
    let _ = conn.shutdown();

    assert_eq!(response["result"]["isError"], json!(true), "{response}");
    let payload = text_payload(&response);
    assert_eq!(payload["error"]["code"], "E_INVALID_ARGS", "{response}");
    assert!(
        payload.get("name").is_none() && payload.get("version").is_none(),
        "a refused call must not also report the identity: {response}"
    );
    let wire = response.to_string();
    assert!(
        !wire.contains("probeKEY") && !wire.contains("probeVALUE"),
        "supplied arguments must not be reflected: {wire}"
    );
}

/// Run one batch session against the shipped binary with stderr captured, so the outer loop's
/// structured log lines (`tool_decision`, `assay.usage.metered`) can be read as emitted.
fn run_batch_session(policy_root: &Path, lines: &[Value]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    for (name, _) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("ASSAY_AUTH_") || upper == "RUST_LOG" {
            command.env_remove(name);
        }
    }
    let mut child = command
        .arg("--policy-root")
        .arg(policy_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CARGO_BIN_EXE_assay-mcp-server");
    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for line in lines {
            writeln!(stdin, "{line}").expect("write stdin line");
        }
    }
    child.wait_with_output().expect("wait for server")
}

/// Structured stderr log lines whose `fields.event` equals `event`.
fn log_events(stderr: &[u8], event: &str) -> Vec<Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| record["fields"]["event"] == event)
        .collect()
}

/// One session through the shipped outer loop: an identity call, then a policy call that is the
/// positive contrast. Returns the captured output after asserting both calls succeeded.
fn identity_then_policy_session() -> Output {
    let root = tempfile::tempdir().expect("temp policy-root");
    std::fs::write(
        root.path().join("policy.yaml"),
        "blocklist:\n  - dangerous_tool\n",
    )
    .expect("write policy");
    let output = run_batch_session(
        root.path(),
        &[
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "server-identity-logs", "version": "1.0"}
                }
            }),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                   "params": {"name": TOOL, "arguments": {}}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                   "params": {"name": "assay_policy_decide",
                              "arguments": {"tool": "shell.exec", "policy": "policy.yaml"}}}),
        ],
    );
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdout line is JSON"))
        .collect();
    let identity = responses
        .iter()
        .find(|r| r["id"] == 2)
        .expect("identity response");
    assert_eq!(identity["result"]["isError"], json!(false), "{identity}");
    let policy = responses
        .iter()
        .find(|r| r["id"] == 3)
        .expect("policy response");
    assert_eq!(policy["result"]["isError"], json!(false), "{policy}");

    output
}

/// The identity tool takes no policy decision, so the outer loop must not record a
/// `tool_decision` for it; the policy tool in the same session still records exactly one.
#[test]
fn identity_call_records_no_policy_decision() {
    let output = identity_then_policy_session();
    let decisions = log_events(&output.stderr, "tool_decision");
    let decided_tools: Vec<String> = decisions
        .iter()
        .map(|record| {
            let decision: Value = serde_json::from_str(
                record["fields"]["decision"]
                    .as_str()
                    .expect("decision is a JSON string"),
            )
            .expect("decision parses");
            decision["tool"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        decided_tools,
        vec!["assay_policy_decide".to_string()],
        "only the policy tool may produce a tool_decision record; the identity tool must not \
         be projected as a deny/blocked decision"
    );
}

/// The identity tool reads no policy, so it must not emit a metered `policy_check`; the policy tool
/// in the same session still emits exactly one.
#[test]
fn identity_call_emits_no_metered_policy_check() {
    let output = identity_then_policy_session();
    let metered = log_events(&output.stderr, "assay.usage.metered");
    assert_eq!(
        metered.len(),
        1,
        "only the policy tool may emit a metered policy_check: {metered:?}"
    );
    assert_eq!(metered[0]["fields"]["usage_type"], "policy_check");
    assert_eq!(metered[0]["fields"]["count"], 1);
}
