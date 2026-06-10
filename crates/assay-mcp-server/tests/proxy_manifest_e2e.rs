//! P61c: MCP upstream proxy mode — manifest-observation. End-to-end tests for the pagination tracker,
//! the emitted artifact (assay.mcp_manifest_observed.v0), and the separate observation-health record.
//! Spec: docs/reference/mcp-upstream-proxy-mode.md.
//!
//! Two invariants are asserted first: a `tools/call` still never reaches the upstream in this mode
//! (no regression of the P61b denial), and the emitted manifest digest equals the committed P60a/P60b
//! digest for the canonical-example tools (the producer the proxy feeds is the same one). Honest
//! completeness is then exercised across complete / partial / unknown / not_observed / ambiguous, and
//! the latest-complete-wins rule is checked together with the observation-health context.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const PROXY_UNSUPPORTED: i64 = -32040;

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn mock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/mock_upstream.py")
}

struct Proxy {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

fn spawn(mode: &str, manifest_out: Option<&Path>, health_out: Option<&Path>) -> Proxy {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.arg("proxy")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u"])
        .args(["--upstream-arg", mock_path().to_str().unwrap()])
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(p) = manifest_out {
        cmd.args(["--mcp-manifest-observed-out", p.to_str().unwrap()]);
    }
    if let Some(p) = health_out {
        cmd.args(["--proxy-observation-health-out", p.to_str().unwrap()]);
    }
    let mut child = cmd.spawn().expect("spawn proxy (is python installed?)");
    let stdin = child.stdin.take().unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    Proxy {
        child,
        stdin: Some(stdin),
        out,
    }
}

fn send(p: &mut Proxy, v: Value) {
    let s = p.stdin.as_mut().unwrap();
    writeln!(s, "{v}").unwrap();
    s.flush().unwrap();
}

/// Read the next JSON-RPC RESPONSE (has an id, no method); skip notifications/requests from upstream.
fn read_response(p: &mut Proxy) -> Value {
    let mut line = String::new();
    loop {
        line.clear();
        let n = p.out.read_line(&mut line).expect("read");
        assert!(n > 0, "proxy closed stdout before responding");
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(t).expect("parse JSON");
        if v.get("method").is_some() {
            continue; // a notification or upstream-initiated request; not our response
        }
        return v;
    }
}

fn init() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    })
}

/// Send initialize and consume its response, so subsequent reads line up with the tools/list traffic.
fn handshake(p: &mut Proxy) {
    send(p, init());
    let _ = read_response(p);
}

/// Drive a tools/list chain from a cursorless start, following nextCursor to the terminal page.
fn drive_full_list(p: &mut Proxy, mut next_id: i64) {
    send(
        p,
        serde_json::json!({"jsonrpc": "2.0", "id": next_id, "method": "tools/list"}),
    );
    loop {
        let r = read_response(p);
        let cursor = r["result"]["nextCursor"].as_str().map(|s| s.to_string());
        match cursor {
            Some(c) => {
                next_id += 1;
                send(
                    p,
                    serde_json::json!({"jsonrpc": "2.0", "id": next_id, "method": "tools/list", "params": {"cursor": c}}),
                );
            }
            None => break,
        }
    }
}

fn shutdown(mut p: Proxy) -> std::process::ExitStatus {
    drop(p.stdin.take()); // client EOF
    p.child.wait().expect("wait")
}

fn read_artifact(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("artifact written")).expect("json")
}

// --- the two anchors, first --------------------------------------------------------------------

#[test]
fn tools_call_still_not_forwarded_in_manifest_mode() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    send(
        &mut p,
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let r = read_response(&mut p);
    assert_eq!(r["error"]["code"], PROXY_UNSUPPORTED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    shutdown(p);
}

#[test]
fn p60a_digest_anchor() {
    // The proxy feeds the same P60b producer, so the emitted manifest_digest for the canonical-example
    // tools equals the committed P60a digest.
    let expected_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift/canonicalization_example.json");
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(expected_path).unwrap()).unwrap();
    let expected_digest = expected["manifest"]["expected_manifest_digest"]
        .as_str()
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("p60a", Some(&manifest), None);
    handshake(&mut p);
    drive_full_list(&mut p, 2);
    shutdown(p);

    let m = read_artifact(&manifest);
    assert_eq!(m["schema"], "assay.mcp_manifest_observed.v0");
    assert_eq!(m["status"], "observed");
    assert_eq!(m["observed"]["tools_list_complete"], "complete");
    assert_eq!(
        m["observed"]["manifest_digest"].as_str().unwrap(),
        expected_digest,
        "proxy-emitted manifest_digest must equal the committed P60a digest"
    );
}

// --- completeness semantics --------------------------------------------------------------------

#[test]
fn single_non_paginated_list_is_complete() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    drive_full_list(&mut p, 2);
    shutdown(p);
    let m = read_artifact(&manifest);
    assert_eq!(m["status"], "observed");
    assert_eq!(m["observed"]["tools_list_complete"], "complete");
    assert_eq!(m["observed"]["tool_count"], 1);
}

#[test]
fn multi_page_chain_is_complete_and_accumulates() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("paginated", Some(&manifest), None);
    handshake(&mut p);
    drive_full_list(&mut p, 2); // follows c1 to the terminal page
    shutdown(p);
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "complete");
    assert_eq!(m["observed"]["tool_count"], 2, "both pages accumulated");
}

#[test]
fn unfinished_chain_at_shutdown_is_partial() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let health = dir.path().join("h.json");
    let mut p = spawn("partial", Some(&manifest), Some(&health));
    handshake(&mut p);
    // Start the chain but do NOT follow the advertised nextCursor.
    send(
        &mut p,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r = read_response(&mut p);
    assert!(r["result"]["nextCursor"].is_string());
    shutdown(p);
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "partial");
    assert_ne!(m["status"], "not_observed");
    let h = read_artifact(&health);
    assert_eq!(
        h["manifest_observation"]["emitted_state_source"],
        "best_partial"
    );
}

#[test]
fn mid_stream_join_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    // First observed tools/list already carries a cursor: the chain start was never observed.
    send(
        &mut p,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"cursor": "joined-midway"}}),
    );
    let _ = read_response(&mut p);
    shutdown(p);
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "unknown");
}

#[test]
fn no_tools_list_writes_not_observed_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    shutdown(p); // never sent tools/list
    let m = read_artifact(&manifest);
    assert_eq!(m["status"], "not_observed", "artifact present, not absent");
    assert!(m["observed"]["manifest_digest"].is_null());
}

#[test]
fn duplicate_tool_names_is_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let health = dir.path().join("h.json");
    let mut p = spawn("duplicate", Some(&manifest), Some(&health));
    handshake(&mut p);
    drive_full_list(&mut p, 2);
    shutdown(p);
    let m = read_artifact(&manifest);
    assert_eq!(m["status"], "ambiguous");
    assert!(m["observed"]["manifest_digest"].is_null());
    let h = read_artifact(&health);
    assert_eq!(
        h["manifest_observation"]["emitted_state_source"],
        "ambiguous"
    );
}

#[test]
fn complete_then_later_partial_keeps_latest_complete_with_health_context() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let health = dir.path().join("h.json");
    let mut p = spawn("complete_then_partial", Some(&manifest), Some(&health));
    handshake(&mut p);
    // First chain: cursorless, terminal -> complete.
    send(
        &mut p,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r1 = read_response(&mut p);
    assert!(r1["result"]["nextCursor"].is_null());
    // Second chain: cursorless start that advertises a next page; do not follow it.
    send(
        &mut p,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
    );
    let r2 = read_response(&mut p);
    assert!(r2["result"]["nextCursor"].is_string());
    shutdown(p);

    let m = read_artifact(&manifest);
    assert_eq!(
        m["observed"]["tools_list_complete"], "complete",
        "latest complete wins"
    );
    assert_eq!(m["observed"]["tool_digests"][0]["name"], "echo");
    let h = read_artifact(&health);
    assert_eq!(
        h["manifest_observation"]["emitted_state_source"],
        "latest_complete"
    );
    assert_eq!(
        h["manifest_observation"]["later_incomplete_chain_observed"],
        true
    );
    assert_eq!(h["manifest_observation"]["observed_list_operations"], 2);
}

#[test]
fn list_changed_notification_is_observed_in_health() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let health = dir.path().join("h.json");
    let mut p = spawn("changed", Some(&manifest), Some(&health));
    handshake(&mut p);
    drive_full_list(&mut p, 2);
    shutdown(p);
    let h = read_artifact(&health);
    assert_eq!(
        h["manifest_observation"]["tools_list_changed_observed"],
        true
    );
}

#[test]
fn artifact_write_failure_exits_nonzero() {
    // Deterministic failure: the output path's parent directory does not exist.
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("does-not-exist").join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    drive_full_list(&mut p, 2);
    let status = shutdown(p);
    assert!(
        !status.success(),
        "a requested-artifact write failure must yield a non-zero exit"
    );
    assert!(!manifest.exists());
}
