//! Real-stdio contract for bounded MCP policy-file ingest (#2389).
//!
//! The five advertised policy tools must refuse a local policy of `limit + 1`
//! bytes with `isError:true`, `allowed:false`, and `E_LIMIT_EXCEEDED` before
//! parse or cache insertion. First/repeat cache-limit cases apply only to
//! `assay_policy_decide` and `assay_check_sequence`; the other three tools
//! have no policy cache. Invalid UTF-8 through `assay_check_args` remains
//! the #2387 parse classification.
//!
//! Inventory on `249e8160`: four tools call `tokio::fs::read`; `assay_check_args`
//! calls `McpPolicy::from_file`, which uses unbounded `std::fs::read` in
//! `legacy.rs` (not `read_to_string`).

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const POLICY_LIMIT: usize = 128;
const YAML_INVALID: &str = "Policy YAML is invalid";

fn spawn_server(policy_root: &Path) -> Conn {
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "--policy-root",
            policy_root.to_str().expect("utf-8 policy-root"),
        ])
        .env("ASSAY_MCP_MAX_POLICY_BYTES", POLICY_LIMIT.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay-mcp-server");
    Conn::attach(child)
}

fn initialize(conn: &mut Conn) {
    conn.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "policy-ingest-limits", "version": "1.0"}
        }),
        1,
    );
}

fn call_tool(conn: &mut Conn, tool: &str, arguments: Value, id: u64) -> (Value, Value) {
    let resp = conn.request(
        "tools/call",
        serde_json::json!({"name": tool, "arguments": arguments}),
        id,
    );
    let result = resp
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("tools/call {id} missing result: {resp}"));
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call {id} missing text: {result}"));
    let body: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tools/call {id} body is not JSON ({e}): {text}"));
    (result, body)
}

fn write_policy(dir: &Path, name: &str, contents: &[u8]) {
    fs::write(dir.join(name), contents).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

fn pad_yaml_to(body: &str, target: usize) -> Vec<u8> {
    let mut out = body.as_bytes().to_vec();
    assert!(
        out.len() < target,
        "fixture body is already {len} bytes; cannot pad to {target}",
        len = out.len()
    );
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    if out.len() < target {
        out.push(b'#');
    }
    while out.len() < target {
        out.push(b'p');
    }
    assert_eq!(out.len(), target);
    out
}

fn assert_limit_exceeded(label: &str, result: &Value, body: &Value) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "{label}: result.isError must be true; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "{label}: allowed must be false; body={body}"
    );
    assert_eq!(
        body.pointer("/error/code").and_then(Value::as_str),
        Some("E_LIMIT_EXCEEDED"),
        "{label}: error.code must be E_LIMIT_EXCEEDED; body={body}"
    );
}

fn policy_decide_args(policy: &str) -> Value {
    serde_json::json!({"tool": "read_file", "policy": policy})
}

fn check_args_args(policy: &str) -> Value {
    serde_json::json!({
        "tool": "read_file",
        "arguments": {"path": "/tmp/safe.txt"},
        "policy": policy
    })
}

fn check_sequence_args(policy: &str) -> Value {
    serde_json::json!({
        "history": [],
        "next_tool": "read_file",
        "policy": policy
    })
}

fn check_coverage_args(policy: &str) -> Value {
    serde_json::json!({
        "policy": policy,
        "traces": [{"tools": ["Search"]}]
    })
}

fn explain_trace_args(policy: &str) -> Value {
    serde_json::json!({
        "policy": policy,
        "trace": [{"tool": "Search"}]
    })
}

#[test]
fn five_tools_refuse_limit_plus_one_policy_file() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    let oversized = POLICY_LIMIT + 1;

    write_policy(
        root,
        "decide-over.yaml",
        &pad_yaml_to("blocklist: []\n", oversized),
    );
    write_policy(
        root,
        "args-over.yaml",
        &pad_yaml_to(
            "version: \"2.0\"\ntools:\n  allow:\n    - read_file\n",
            oversized,
        ),
    );
    write_policy(
        root,
        "seq-over.yaml",
        &pad_yaml_to("- read_file\n", oversized),
    );
    write_policy(
        root,
        "cov-over.yaml",
        &pad_yaml_to(
            "version: \"1.1\"\nname: t\ntools:\n  allow: [Search]\nsequences: []\n",
            oversized,
        ),
    );
    write_policy(
        root,
        "explain-over.yaml",
        &pad_yaml_to("version: \"1.1\"\nname: t\nsequences: []\n", oversized),
    );

    let mut conn = spawn_server(root);
    initialize(&mut conn);

    let cases = [
        (
            "assay_policy_decide",
            policy_decide_args("decide-over.yaml"),
            2u64,
        ),
        ("assay_check_args", check_args_args("args-over.yaml"), 3),
        (
            "assay_check_sequence",
            check_sequence_args("seq-over.yaml"),
            4,
        ),
        (
            "assay_check_coverage",
            check_coverage_args("cov-over.yaml"),
            5,
        ),
        (
            "assay_explain_trace",
            explain_trace_args("explain-over.yaml"),
            6,
        ),
    ];

    for (tool, arguments, id) in cases {
        let (result, body) = call_tool(&mut conn, tool, arguments, id);
        assert_limit_exceeded(tool, &result, &body);
    }

    // Cache-bearing tools only: a second oversized call must still be a limit
    // failure, proving the first call did not insert a compiled policy.
    let (decide_repeat, decide_repeat_body) = call_tool(
        &mut conn,
        "assay_policy_decide",
        policy_decide_args("decide-over.yaml"),
        7,
    );
    assert_limit_exceeded(
        "assay_policy_decide repeat",
        &decide_repeat,
        &decide_repeat_body,
    );
    let (seq_repeat, seq_repeat_body) = call_tool(
        &mut conn,
        "assay_check_sequence",
        check_sequence_args("seq-over.yaml"),
        8,
    );
    assert_limit_exceeded("assay_check_sequence repeat", &seq_repeat, &seq_repeat_body);

    let _ = conn.shutdown();
}

fn assert_not_limit_error(label: &str, result: &Value, body: &Value) {
    assert_ne!(
        body.pointer("/error/code").and_then(Value::as_str),
        Some("E_LIMIT_EXCEEDED"),
        "{label}: exact-limit file must not be a limit failure; result={result} body={body}"
    );
}

#[test]
fn five_tools_accept_exact_limit_valid_policy_files() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();

    write_policy(
        root,
        "decide-exact.yaml",
        &pad_yaml_to("blocklist: []\n", POLICY_LIMIT),
    );
    write_policy(
        root,
        "args-exact.yaml",
        &pad_yaml_to(
            "version: \"2.0\"\ntools:\n  allow:\n    - read_file\n",
            POLICY_LIMIT,
        ),
    );
    write_policy(
        root,
        "seq-exact.yaml",
        &pad_yaml_to("- read_file\n", POLICY_LIMIT),
    );
    write_policy(
        root,
        "cov-exact.yaml",
        &pad_yaml_to(
            "version: \"1.1\"\nname: t\ntools:\n  allow: [Search]\nsequences: []\n",
            POLICY_LIMIT,
        ),
    );
    write_policy(
        root,
        "explain-exact.yaml",
        &pad_yaml_to("version: \"1.1\"\nname: t\nsequences: []\n", POLICY_LIMIT),
    );

    let mut conn = spawn_server(root);
    initialize(&mut conn);

    let (decide_result, decide_body) = call_tool(
        &mut conn,
        "assay_policy_decide",
        policy_decide_args("decide-exact.yaml"),
        2,
    );
    assert_not_limit_error("assay_policy_decide exact", &decide_result, &decide_body);
    assert_eq!(
        decide_body.get("allowed").and_then(Value::as_bool),
        Some(true),
        "assay_policy_decide exact: {decide_body}"
    );

    let (args_result, args_body) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("args-exact.yaml"),
        3,
    );
    assert_not_limit_error("assay_check_args exact", &args_result, &args_body);
    assert_eq!(
        args_body.get("allowed").and_then(Value::as_bool),
        Some(true),
        "assay_check_args exact: {args_body}"
    );

    let (seq_result, seq_body) = call_tool(
        &mut conn,
        "assay_check_sequence",
        check_sequence_args("seq-exact.yaml"),
        4,
    );
    assert_not_limit_error("assay_check_sequence exact", &seq_result, &seq_body);
    assert_eq!(
        seq_body.get("allowed").and_then(Value::as_bool),
        Some(true),
        "assay_check_sequence exact: {seq_body}"
    );

    let (cov_result, cov_body) = call_tool(
        &mut conn,
        "assay_check_coverage",
        check_coverage_args("cov-exact.yaml"),
        5,
    );
    assert_not_limit_error("assay_check_coverage exact", &cov_result, &cov_body);
    assert!(
        cov_body.get("overall_coverage_pct").is_some() || cov_body.get("meets_threshold").is_some(),
        "assay_check_coverage exact must reach the coverage report; body={cov_body}"
    );

    let (explain_result, explain_body) = call_tool(
        &mut conn,
        "assay_explain_trace",
        explain_trace_args("explain-exact.yaml"),
        6,
    );
    assert_not_limit_error("assay_explain_trace exact", &explain_result, &explain_body);
    assert!(
        explain_body.get("total_steps").is_some() || explain_body.get("blocked_steps").is_some(),
        "assay_explain_trace exact must reach the explanation; body={explain_body}"
    );

    let _ = conn.shutdown();
}

#[test]
fn check_args_invalid_utf8_stays_policy_parse() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(root, "bad-utf8.yaml", &[0xFF, 0xFE, b'v', b':', b' ', b'1']);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("bad-utf8.yaml"),
        2,
    );

    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "invalid-utf8: result.isError must be true; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "invalid-utf8: allowed must be false; body={body}"
    );
    assert_eq!(
        body.pointer("/error/code").and_then(Value::as_str),
        Some("E_POLICY_PARSE"),
        "invalid-utf8: error.code must be E_POLICY_PARSE; body={body}"
    );
    assert_eq!(
        body.pointer("/error/message").and_then(Value::as_str),
        Some(YAML_INVALID),
        "invalid-utf8: message must stay Policy YAML is invalid; body={body}"
    );

    let _ = conn.shutdown();
}
