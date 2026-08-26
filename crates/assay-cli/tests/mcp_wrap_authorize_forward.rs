//! README-facing `assay mcp wrap`: the value authorized must be the value forwarded.
//!
//! Abstract member / malformed-frame fixtures only. Not a named-upstream bypass recipe.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "../../assay-mcp-server/tests/jsonrpc_conn/mod.rs"]
mod jsonrpc_conn;

use jsonrpc_conn::Conn;

fn assay_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_assay"))
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn oracle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assay-mcp-server/tests/fixtures/proxy/member_oracles.py")
}

fn write_policy(dir: &Path) -> PathBuf {
    let p = dir.join("wrap-policy.yaml");
    std::fs::write(
        &p,
        r#"
version: "2.0"
name: "authorize-forward"
tools:
  allow: ["echo"]
enforcement:
  unconstrained_tools: allow
"#,
    )
    .expect("write policy");
    p
}

fn spawn_wrap(dir: &Path, mode: &str) -> (Conn, PathBuf, PathBuf, PathBuf) {
    let policy = write_policy(dir);
    let raw = dir.join("raw.log");
    let interpret = dir.join("interpret.ndjson");
    let decisions = dir.join("decisions.ndjson");
    let child = Command::new(assay_bin())
        .args([
            "mcp",
            "wrap",
            "--policy",
            policy.to_str().unwrap(),
            "--decision-log",
            decisions.to_str().unwrap(),
            "--event-source",
            "assay://tests/authorize-forward",
            "--",
            python(),
            "-u",
            oracle_path().to_str().unwrap(),
            "serve",
            mode,
        ])
        .current_dir(dir)
        .env("ORACLE_RAW_LOG", "raw.log")
        .env("ORACLE_INTERPRET_LOG", "interpret.ndjson")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay mcp wrap");
    (Conn::attach(child), raw, interpret, decisions)
}

fn last_interpret(path: &Path) -> Option<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("interpret json"))
        .next_back()
}

fn last_decision_tool(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            v.pointer("/data/tool")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .next_back()
}

fn raw_contains_tools_call(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .contains("tools/call")
}

fn tool_decision_count(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("type").and_then(Value::as_str) == Some("assay.tool.decision"))
        .count()
}

#[test]
fn wrap_unique_key_control_authorized_equals_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, interpret, decisions) = spawn_wrap(dir.path(), "last");
    let line = r#"{"jsonrpc":"2.0","id":1e2,"method":"tools/call","params":{"name":"echo","arguments":{"msg":"ok"}}}"#;
    conn.send_line(line);
    let r = conn.read_response();
    assert!(r.get("error").is_none(), "unique-key wrap denied: {r}");
    let _ = conn.shutdown();

    assert!(
        raw_contains_tools_call(&raw),
        "unique-key control must reach upstream"
    );
    assert!(
        std::fs::read_to_string(&raw).unwrap().contains("1e2"),
        "wrap must preserve the written numeric id"
    );
    let seen = last_interpret(&interpret).expect("oracle interpretation");
    assert_eq!(seen["accepted"], true);
    assert_eq!(seen["name"], "echo");
    assert_eq!(seen["arguments"], serde_json::json!({"msg": "ok"}));
    assert_eq!(last_decision_tool(&decisions).as_deref(), Some("echo"));
}

const INIT_LINE: &str = r#"{"jsonrpc":"2.0","id":21,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
const LIST_LINE: &str = r#"{"jsonrpc":"2.0","id":22,"method":"tools/list"}"#;
const NOTE_LINE: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const NON_REQUEST_LINE: &str = r#"{"jsonrpc":"2.0","id":23,"result":{"passthrough":true}}"#;
const PING_LINE: &str = r#"{"jsonrpc":"2.0","id":24,"method":"ping"}"#;

#[test]
fn wrap_protocol_control_is_forwarded_without_tool_decision() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, _, decisions) = spawn_wrap(dir.path(), "last");

    conn.send_line(INIT_LINE);
    let init = conn.read_response_for_id(21);
    assert!(
        init.get("error").is_none(),
        "initialize must be forwarded, not policy-denied: {init}"
    );
    assert_eq!(
        init["result"]["protocolVersion"], "2024-11-05",
        "initialize must reach the oracle: {init}"
    );
    assert_eq!(
        tool_decision_count(&decisions),
        0,
        "initialize must not mint assay.tool.decision: {}",
        std::fs::read_to_string(&decisions).unwrap_or_default()
    );

    conn.send_line(LIST_LINE);
    let listed = conn.read_response_for_id(22);
    assert!(
        listed.get("error").is_none(),
        "tools/list must be forwarded, not policy-denied: {listed}"
    );
    assert_eq!(
        tool_decision_count(&decisions),
        0,
        "tools/list must not mint assay.tool.decision: {}",
        std::fs::read_to_string(&decisions).unwrap_or_default()
    );

    conn.send_line(NOTE_LINE);
    conn.send_line(NON_REQUEST_LINE);
    conn.send_line(PING_LINE);
    let ping = conn.read_response_for_id(24);
    assert!(
        ping.get("error").is_none(),
        "ping must be forwarded, not policy-denied: {ping}"
    );
    assert_eq!(
        tool_decision_count(&decisions),
        0,
        "non-tools/call unique objects must not mint assay.tool.decision: {}",
        std::fs::read_to_string(&decisions).unwrap_or_default()
    );

    conn.send_line(
        r#"{"jsonrpc":"2.0","id":25,"method":"tools/call","params":{"name":"echo","arguments":{"msg":"ok"}}}"#,
    );
    let call = conn.read_response_for_id(25);
    assert!(
        call.get("error").is_none(),
        "tools/call must still authorize the exact parsed value: {call}"
    );
    let _ = conn.shutdown();

    let body = std::fs::read_to_string(&raw).unwrap_or_default();
    for line in [INIT_LINE, LIST_LINE, NOTE_LINE, NON_REQUEST_LINE, PING_LINE] {
        assert!(
            body.contains(line),
            "protocol-control line must be forwarded byte-identically: missing {line} in {body}"
        );
    }
    assert!(
        body.contains("\"method\":\"tools/call\""),
        "tools/call must still reach upstream: {body}"
    );
    assert_eq!(
        last_decision_tool(&decisions).as_deref(),
        Some("echo"),
        "only the tools/call is authorized"
    );
    assert_eq!(
        tool_decision_count(&decisions),
        1,
        "exactly one assay.tool.decision, for the tools/call: {}",
        std::fs::read_to_string(&decisions).unwrap_or_default()
    );
}

#[test]
fn wrap_noop_control_unique_ping_is_not_a_tools_call() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, _, _) = spawn_wrap(dir.path(), "last");
    conn.send_line(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#);
    let r = conn.read_response();
    assert_eq!(r["id"], 7);
    let _ = conn.shutdown();
    assert!(!raw_contains_tools_call(&raw));
}

#[test]
fn wrap_duplicate_params_name_is_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, interpret, decisions) = spawn_wrap(dir.path(), "first");
    conn.send_line(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"other","name":"echo","arguments":{}}}"#,
    );
    let _ = conn.read_json();
    let _ = conn.shutdown();
    assert!(
        !raw_contains_tools_call(&raw),
        "duplicate params.name must not be forwarded: {}",
        std::fs::read_to_string(&raw).unwrap_or_default()
    );
    if let Some(seen) = last_interpret(&interpret) {
        assert_ne!(
            seen["accepted"], true,
            "first-member oracle must not see a forwarded call"
        );
    }
    if let Some(tool) = last_decision_tool(&decisions) {
        panic!("must not authorize a collapsed tree, got tool={tool}");
    }
}

#[test]
fn wrap_duplicate_argument_member_is_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, _, decisions) = spawn_wrap(dir.path(), "first");
    conn.send_line(
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"echo","arguments":{"msg":"other","msg":"ok"}}}"#,
    );
    let _ = conn.read_json();
    let _ = conn.shutdown();
    assert!(
        !raw_contains_tools_call(&raw),
        "duplicate argument members must not be forwarded: {}",
        std::fs::read_to_string(&raw).unwrap_or_default()
    );
    if let Some(tool) = last_decision_tool(&decisions) {
        panic!("must not authorize a collapsed tree, got tool={tool}");
    }
}

/// Abstract duplicate-member fixture whose keys are JSON-escaped. It contains
/// no literal `"method"` / `"params"` / `"tool"` token, so the raw
/// method-bearing heuristic does not see it.
const ESCAPED_DUPLICATE_OBJECT: &str = r#"{"jsonrpc":"2.0","id":1,"\u006dethod":"tools/list","\u006dethod":"tools/call","\u0070arams":{"name":"danger","arguments":{}}}"#;

#[test]
fn wrap_escaped_duplicate_object_is_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, interpret, decisions) = spawn_wrap(dir.path(), "last");
    conn.send_line(ESCAPED_DUPLICATE_OBJECT);
    let r = conn.read_json();
    assert_eq!(
        r["error"]["code"], -32700,
        "object-shaped unique-parse fail must be a parse error, not a forwarded result: {r}"
    );
    conn.send_line(
        r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"echo","arguments":{}}}"#,
    );
    let follow = conn.read_response_for_id(11);
    assert!(
        follow.get("error").is_none(),
        "follow-up unique call must still work: {follow}"
    );
    let _ = conn.shutdown();

    let body = std::fs::read_to_string(&raw).unwrap_or_default();
    assert!(
        !body.contains(ESCAPED_DUPLICATE_OBJECT),
        "escaped duplicate object must not reach the oracle: {body}"
    );
    assert!(
        !body.contains(r#"\u006dethod"#),
        "escaped duplicate object must not reach the oracle: {body}"
    );
    if let Some(seen) = last_interpret(&interpret) {
        assert_ne!(
            seen["name"], "danger",
            "first/last oracle must not see a forwarded escaped duplicate: {seen}"
        );
    }
    if let Some(tool) = last_decision_tool(&decisions) {
        assert_ne!(
            tool, "danger",
            "must not authorize a collapsed tree, got tool={tool}"
        );
    }
}

#[test]
fn wrap_unparsable_method_bearing_frame_is_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, raw, _, _) = spawn_wrap(dir.path(), "reject");
    let broken = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"echo""#;
    conn.send_line(broken);
    conn.send_line(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"echo","arguments":{}}}"#);
    let r = conn.read_response_for_id(10);
    assert!(
        r.get("error").is_none(),
        "follow-up unique call must still work: {r}"
    );
    let _ = conn.shutdown();
    let body = std::fs::read_to_string(&raw).unwrap_or_default();
    assert!(
        !body.contains(broken),
        "unparsable method-bearing frame must not be forwarded: {body}"
    );
    assert!(
        body.contains("\"id\":10"),
        "follow-up unique call is the no-op control that the stream stayed intact"
    );
}
