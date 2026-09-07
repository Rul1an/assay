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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const PROXY_UNSUPPORTED: i64 = -31997;

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

fn spawn(mode: &str, manifest_out: Option<&Path>, health_out: Option<&Path>) -> Conn {
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
    Conn::attach(cmd.spawn().expect("spawn proxy (is python installed?)"))
}

fn init() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    })
}

/// Send initialize and consume its response, so subsequent reads line up with the tools/list traffic.
fn handshake(p: &mut Conn) {
    p.send(init());
    let _ = p.read_response();
}

/// Drive a tools/list chain from a cursorless start, following nextCursor to the terminal page.
fn drive_full_list(p: &mut Conn, mut next_id: i64) {
    p.send(serde_json::json!({"jsonrpc": "2.0", "id": next_id, "method": "tools/list"}));
    loop {
        let r = p.read_response();
        let cursor = r["result"]["nextCursor"].as_str().map(|s| s.to_string());
        match cursor {
            Some(c) => {
                next_id += 1;
                p.send(
                    serde_json::json!({"jsonrpc": "2.0", "id": next_id, "method": "tools/list", "params": {"cursor": c}}),
                );
            }
            None => break,
        }
    }
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
    p.send(
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let r = p.read_response();
    assert_eq!(r["error"]["code"], PROXY_UNSUPPORTED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    let _ = p.shutdown();
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
    let _ = p.shutdown();

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
    let _ = p.shutdown();
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
    let _ = p.shutdown();
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
    p.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let r = p.read_response();
    assert!(r["result"]["nextCursor"].is_string());
    let _ = p.shutdown();
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
    p.send(
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"cursor": "joined-midway"}}),
    );
    let _ = p.read_response();
    let _ = p.shutdown();
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "unknown");
}

#[test]
fn no_tools_list_writes_not_observed_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("m.json");
    let mut p = spawn("normal", Some(&manifest), None);
    handshake(&mut p);
    let _ = p.shutdown(); // never sent tools/list
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
    let _ = p.shutdown();
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
    p.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let r1 = p.read_response();
    assert!(r1["result"]["nextCursor"].is_null());
    // Second chain: cursorless start that advertises a next page; do not follow it.
    p.send(serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}));
    let r2 = p.read_response();
    assert!(r2["result"]["nextCursor"].is_string());
    let _ = p.shutdown();

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
    let _ = p.shutdown();
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
    let status = p.shutdown();
    assert!(
        !status.success(),
        "a requested-artifact write failure must yield a non-zero exit"
    );
    assert!(!manifest.exists());
}

// #2840: finite scripted responses drive the real proxy observer and artifact writer.
fn spawn_list_responses(pages: &[Value], manifest: &Path, health: &Path) -> Conn {
    let script = r#"
import json, os, sys
pages = iter(json.loads(os.environ['ASSAY_TEST_LIST_RESPONSES']))
for line in sys.stdin:
    request = json.loads(line)
    if request.get('method') == 'initialize':
        response = {'result': {'protocolVersion': request['params']['protocolVersion'],
                    'capabilities': {}, 'serverInfo': {'name': 'scripted', 'version': '1'}}}
    elif request.get('method') == 'tools/list':
        response = next(pages)
    else:
        continue
    response.update({'jsonrpc': '2.0', 'id': request['id']})
    print(json.dumps(response), flush=True)
"#;
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .arg("proxy")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u", "--upstream-arg", "-c"])
        .args(["--upstream-arg", script])
        .args(["--mcp-manifest-observed-out", manifest.to_str().unwrap()])
        .args(["--proxy-observation-health-out", health.to_str().unwrap()])
        .env(
            "ASSAY_TEST_LIST_RESPONSES",
            serde_json::to_string(pages).unwrap(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn proxy with finite scripted upstream");
    Conn::attach(child)
}

fn list_response(p: &mut Conn, id: u64, cursor: Option<&str>) -> Value {
    let params = cursor.map_or_else(
        || serde_json::json!({}),
        |c| serde_json::json!({"cursor": c}),
    );
    p.request("tools/list", params, id)
}

fn scripted_tool() -> Value {
    serde_json::json!({"name": "retained", "description": "test", "inputSchema": {"type": "object"}})
}

#[test]
fn tools_list_error_cannot_emit_complete_catalogue() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    let health = dir.path().join("health.json");
    let error = serde_json::json!({"error": {"code": -32603, "message": "request failed"}});
    let mut p = spawn_list_responses(std::slice::from_ref(&error), &manifest, &health);
    handshake(&mut p);
    let response = list_response(&mut p, 2, None);
    let midrun_exists = manifest.try_exists();
    let exit = p.shutdown();
    assert!(exit.success());
    assert_eq!(response["error"], error["error"]);
    assert!(
        !midrun_exists.expect("inspect mid-run artifact"),
        "failed tools/list must not emit a complete manifest mid-run"
    );
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "partial");
    assert_eq!(m["observed"]["tool_count"], 0);
    assert_eq!(
        read_artifact(&health)["manifest_observation"]["emitted_state_source"],
        "best_partial"
    );
}

#[test]
fn tools_list_malformed_page_cannot_emit_complete_catalogue() {
    let malformed = [
        serde_json::json!({}),
        serde_json::json!({"result": null}),
        serde_json::json!({"result": {}}),
        serde_json::json!({"result": {"tools": null}}),
        serde_json::json!({"result": {"tools": "wrong"}}),
        serde_json::json!({"result": {"tools": {}}}),
        serde_json::json!({"result": {"tools": []}, "error": {"code": -32603, "message": "failed"}}),
        serde_json::json!({"result": {"tools": []}, "error": null}),
    ];
    for (case, page) in malformed.into_iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("manifest.json");
        let health = dir.path().join("health.json");
        let mut p = spawn_list_responses(std::slice::from_ref(&page), &manifest, &health);
        handshake(&mut p);
        let response = list_response(&mut p, 2, None);
        let midrun_exists = manifest.try_exists();
        let exit = p.shutdown();
        assert!(exit.success());
        assert_eq!(
            response["id"], 2,
            "case {case} must reach the real observer and relay"
        );
        for (key, value) in page.as_object().unwrap() {
            assert_eq!(
                &response[key], value,
                "malformed response remains transparent, case {case}"
            );
        }
        assert!(
            !midrun_exists.expect("inspect mid-run artifact"),
            "malformed tools/list must not emit complete, case {case}"
        );
        assert_eq!(
            read_artifact(&manifest)["observed"]["tools_list_complete"],
            "partial",
            "case {case}"
        );
    }
}

#[test]
fn tools_list_successful_empty_page_is_complete() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    let health = dir.path().join("health.json");
    let mut p = spawn_list_responses(
        &[serde_json::json!({"result": {"tools": []}})],
        &manifest,
        &health,
    );
    handshake(&mut p);
    let response = list_response(&mut p, 2, None);
    let midrun_bytes = std::fs::read(&manifest);
    let exit = p.shutdown();
    assert!(exit.success());
    assert_eq!(response["result"]["tools"], serde_json::json!([]));
    let m: Value = serde_json::from_slice(&midrun_bytes.expect("mid-run complete artifact"))
        .expect("mid-run JSON");
    assert_eq!(m["status"], "observed");
    assert_eq!(m["observed"]["tools_list_complete"], "complete");
    assert_eq!(m["observed"]["tool_count"], 0);
    assert!(m["observed"]["manifest_digest"].is_string());
    assert_eq!(read_artifact(&manifest), m);
}

#[test]
fn tools_list_second_page_error_keeps_partial_catalogue() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    let health = dir.path().join("health.json");
    let pages = [
        serde_json::json!({"result": {"tools": [scripted_tool()], "nextCursor": "next"}}),
        serde_json::json!({"error": {"code": -32603, "message": "page failed"}}),
    ];
    let mut p = spawn_list_responses(&pages, &manifest, &health);
    handshake(&mut p);
    let first = list_response(&mut p, 2, None);
    let after_first_exists = manifest.try_exists();
    let second = list_response(&mut p, 3, Some("next"));
    let after_error_exists = manifest.try_exists();
    let exit = p.shutdown();
    assert!(exit.success());
    assert_eq!(first["result"]["nextCursor"], "next");
    assert!(!after_first_exists.expect("inspect first-page artifact"));
    assert_eq!(second["error"]["code"], -32603);
    assert!(
        !after_error_exists.expect("inspect mid-run artifact"),
        "page2 error must not finalize prior pages as complete"
    );
    let m = read_artifact(&manifest);
    assert_eq!(m["observed"]["tools_list_complete"], "partial");
    assert_eq!(m["observed"]["tool_count"], 1);
    assert_eq!(m["observed"]["tool_digests"][0]["name"], "retained");
}

#[test]
fn tools_list_failed_refresh_preserves_prior_complete_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    let health = dir.path().join("health.json");
    let pages = [
        serde_json::json!({"result": {"tools": [scripted_tool()]}}),
        serde_json::json!({"error": {"code": -32603, "message": "refresh failed"}}),
    ];
    let mut p = spawn_list_responses(&pages, &manifest, &health);
    handshake(&mut p);
    let _ = list_response(&mut p, 2, None);
    let prior_bytes = std::fs::read(&manifest);
    let response = list_response(&mut p, 3, None);
    let after_error_bytes = std::fs::read(&manifest);
    let exit = p.shutdown();
    assert!(exit.success());
    let prior_bytes = prior_bytes.expect("initial complete artifact");
    let prior: Value = serde_json::from_slice(&prior_bytes).expect("initial JSON");
    assert_eq!(prior["observed"]["tools_list_complete"], "complete");
    assert_eq!(prior["observed"]["tool_count"], 1);
    assert_eq!(response["error"]["code"], -32603);
    assert_eq!(
        after_error_bytes.expect("mid-run preserved artifact"),
        prior_bytes,
        "failed refresh must preserve complete snapshot bytes"
    );
    assert_eq!(
        read_artifact(&manifest),
        prior,
        "latest complete remains a prior snapshot, not a successful refresh"
    );
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
fn tools_list_malformed_cursor_is_not_an_admitted_page() {
    for cursor in [
        Value::Null,
        serde_json::json!(false),
        serde_json::json!(1),
        serde_json::json!({}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("manifest.json");
        let health = dir.path().join("health.json");
        let page =
            serde_json::json!({"result": {"tools": [scripted_tool()], "nextCursor": cursor}});
        let mut p = spawn_list_responses(std::slice::from_ref(&page), &manifest, &health);
        handshake(&mut p);
        let response = list_response(&mut p, 2, None);
        let midrun_exists = manifest.try_exists();
        let exit = p.shutdown();
        assert!(exit.success());
        assert_eq!(response["result"], page["result"]);
        assert!(
            !midrun_exists.expect("inspect mid-run artifact"),
            "null cursor must not masquerade as a terminal page"
        );
        let m = read_artifact(&manifest);
        assert_eq!(m["observed"]["tools_list_complete"], "partial");
        assert_eq!(
            m["observed"]["tool_count"], 0,
            "a malformed cursor page must not contribute tools, including non-null wrong types"
        );
    }
}
