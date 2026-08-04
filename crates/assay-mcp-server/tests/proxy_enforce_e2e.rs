//! P61e-c1: MCP upstream ENFORCING proxy mode — the caller-allowance PDP. End-to-end tests.
//! Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! These were the P61e-b deny-all tests; c1 retired the single `enforcing_mode_deny_all` reason for
//! the per-gate PDP reasons, and `proxy-enforce` now requires `--enforce-policy`. The load-bearing
//! invariant is unchanged and asserted first: in `proxy-enforce` mode a `tools/call` is denied with
//! `proxy_denied` and NEVER reaches the upstream. The two error codes stay distinct: `proxy_denied` is
//! the enforcing-mode policy denial for `tools/call`; `proxy_unsupported` remains for non-allowlisted
//! non-`tools/call` methods. The shipped `proxy` (observe) mode is unchanged. There is still no allow
//! path and no forwarding of `tools/call` in this slice — a call that passes every c1 gate is denied
//! with `pdp_gate_unavailable`.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const PROXY_UNSUPPORTED: i64 = -32040;
const PROXY_DENIED: i64 = -32042;

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

/// A minimal valid enforce policy: one caller, no allowances. Enough for the load-bearing deny tests —
/// `echo` is an unclassified tool, so it denies regardless of allowances.
const MINIMAL_POLICY: &str = "caller:\n  id: \"test-agent\"\nallowances: []\n";

fn write_policy(dir: &std::path::Path, yaml: &str) -> PathBuf {
    let p = dir.join("enforce.yaml");
    std::fs::write(&p, yaml).expect("write policy");
    p
}

/// Spawn the observe proxy ("proxy" subcommand).
fn spawn_observe(log: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            mock_path().to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn observe proxy (is python installed?)")
}

/// The committed approved baseline (`assay.declared_mcp_manifest.v0`). Enforcing mode requires
/// `--declared-mcp-manifest`; these deny tests never reach the drift gate (echo is unclassified), so
/// any valid baseline suffices.
fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json")
}

/// Spawn the enforcing proxy ("proxy-enforce" subcommand) with the given policy + the required baseline.
fn spawn_enforce(log: &std::path::Path, policy: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            mock_path().to_str().unwrap(),
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            baseline_path().to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

fn init() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    })
}

fn read_methods(log: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

// --- the load-bearing test, first ---------------------------------------------------------------

#[test]
fn enforcing_mode_tools_call_denied_and_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_policy(dir.path(), MINIMAL_POLICY);
    let mut out = Conn::attach(spawn_enforce(&log, &policy));

    out.send(init());
    let _ = out.read_response();
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let _ = out.read_response();

    out.send(
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let r = out.read_response();
    assert_eq!(r["id"], 3);
    assert_eq!(
        r["error"]["code"], PROXY_DENIED,
        "tools/call is a policy denial in enforce mode"
    );
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    // `echo` is not a privileged classifier target, so the classification gate fires first.
    assert_eq!(
        r["error"]["data"]["reason"], "unclassified_tool_call",
        "an unclassified tool denies at the classification gate"
    );

    let _ = out.shutdown();

    let methods = read_methods(&log);
    assert!(methods.contains(&"initialize".to_string()));
    assert!(methods.contains(&"tools/list".to_string()));
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "INVARIANT VIOLATED: tools/call reached the upstream in enforce mode: {methods:?}"
    );
}

#[test]
fn enforcing_mode_unknown_method_is_unsupported_not_denied() {
    // A non-allowlisted, non-tools/call method stays proxy_unsupported even in enforce mode — the two
    // codes are distinct: proxy_denied is only for the tools/call policy denial.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_policy(dir.path(), MINIMAL_POLICY);
    let mut out = Conn::attach(spawn_enforce(&log, &policy));

    out.send(init());
    let _ = out.read_response();
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list"}));
    let r = out.read_response();
    assert_eq!(
        r["error"]["code"], PROXY_UNSUPPORTED,
        "non-tools/call stays unsupported, not denied"
    );
    assert_eq!(r["error"]["data"]["reason"], "method_not_allowlisted");

    let _ = out.shutdown();
    assert!(!read_methods(&log).contains(&"resources/list".to_string()));
}

#[test]
fn enforcing_mode_list_methods_still_forward() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_policy(dir.path(), MINIMAL_POLICY);
    let mut out = Conn::attach(spawn_enforce(&log, &policy));

    out.send(init());
    let r = out.read_response();
    assert_eq!(
        r["result"]["serverInfo"]["name"], "mock-upstream",
        "initialize relayed"
    );
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let r = out.read_response();
    assert!(r["result"]["tools"].is_array(), "tools/list relayed");

    let _ = out.shutdown();
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"initialize".to_string()) && methods.contains(&"tools/list".to_string())
    );
}

#[test]
fn observe_mode_tools_call_still_unsupported() {
    // The shipped observe mode is unchanged: a tools/call is proxy_unsupported, NOT proxy_denied.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut out = Conn::attach(spawn_observe(&log));

    out.send(init());
    let _ = out.read_response();
    out.send(
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let r = out.read_response();
    assert_eq!(r["error"]["code"], PROXY_UNSUPPORTED);
    assert_eq!(r["error"]["data"]["reason"], "method_not_allowlisted");
    let _ = out.shutdown();
}

#[test]
fn existing_proxy_invocation_still_observes() {
    // The shipped `proxy --upstream-command ...` invocation is untouched and still observes: the
    // handshake and tools/list reach the upstream, tools/call does not.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut out = Conn::attach(spawn_observe(&log));

    out.send(init());
    let _ = out.read_response();
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let _ = out.read_response();
    out.send(
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "echo"}}),
    );
    let _ = out.read_response();

    let _ = out.shutdown();
    let methods = read_methods(&log);
    assert!(methods.contains(&"initialize".to_string()));
    assert!(methods.contains(&"tools/list".to_string()));
    assert!(!methods.contains(&"tools/call".to_string()));
}
