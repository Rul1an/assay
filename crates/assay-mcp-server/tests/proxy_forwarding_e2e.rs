//! P61b: MCP upstream proxy mode — manifest-observation v0. End-to-end forwarding-skeleton tests.
//! Spec: docs/reference/mcp-upstream-proxy-mode.md.
//!
//! The load-bearing invariant, asserted first: a `tools/call` sent in proxy mode is denied with a
//! proxy-originated error and NEVER reaches the upstream. The upstream is a deterministic stdio mock
//! (tests/fixtures/proxy/mock_upstream.py) that records every method it receives, so "the upstream
//! received nothing" is a checkable fact, not an assumption.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const PROXY_UNSUPPORTED: i64 = -32040;
const PROXY_FAILED: i64 = -32041;

fn python() -> &'static str {
    // GitHub-hosted Windows exposes `python`; Linux/macOS expose `python3`.
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn mock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/mock_upstream.py")
}

/// Spawn the proxy against the python mock upstream. `mode` is the mock's MOCK_UPSTREAM_MODE.
/// `log` records received methods; `raw_log` (optional) records raw received lines.
fn spawn_proxy(log: &std::path::Path, raw_log: Option<&std::path::Path>, mode: &str) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.args([
        "proxy",
        "--upstream-command",
        python(),
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        mock_path().to_str().unwrap(),
    ])
    .env("MOCK_UPSTREAM_LOG", log)
    .env("MOCK_UPSTREAM_MODE", mode)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit());
    if let Some(rl) = raw_log {
        cmd.env("MOCK_UPSTREAM_RAW_LOG", rl);
    }
    cmd.spawn().expect("spawn proxy (is python installed?)")
}

fn send(stdin: &mut ChildStdin, v: Value) {
    writeln!(stdin, "{v}").expect("write request");
    stdin.flush().expect("flush request");
}

/// Read the next non-empty JSON line from the proxy's stdout.
fn read_response(reader: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("read response");
        assert!(n > 0, "proxy closed stdout before responding");
        if !line.trim().is_empty() {
            return serde_json::from_str(line.trim()).expect("parse response JSON");
        }
    }
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

fn shutdown(mut child: Child, stdin: ChildStdin) {
    drop(stdin); // client EOF
    let _ = child.wait();
}

// --- the load-bearing test, first ---------------------------------------------------------------

#[test]
fn tools_call_never_reaches_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let r = read_response(&mut out);
    assert_eq!(r["result"]["serverInfo"]["name"], "mock-upstream");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r = read_response(&mut out);
    assert!(r["result"]["tools"].is_array());

    // The denied call: the proxy must answer proxy_unsupported and the upstream must never see it.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["id"], 3);
    assert_eq!(r["error"]["code"], PROXY_UNSUPPORTED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");

    shutdown(child, stdin);

    let methods = read_methods(&log);
    assert!(
        methods.contains(&"initialize".to_string()),
        "upstream saw initialize"
    );
    assert!(
        methods.contains(&"tools/list".to_string()),
        "upstream saw tools/list"
    );
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "INVARIANT VIOLATED: tools/call reached the upstream: {methods:?}"
    );
}

#[test]
fn non_allowlisted_method_is_denied_and_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list"}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_UNSUPPORTED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");

    shutdown(child, stdin);
    assert!(!read_methods(&log).contains(&"resources/list".to_string()));
}

#[test]
fn proxy_does_not_inject_inbound_transport_auth() {
    // Option 1 (verbatim forwarding): the proxy injects no Authorization/header/token of its own. We
    // send a clean initialize and assert the upstream received exactly those bytes — nothing added.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let mut child = spawn_proxy(&log, Some(&raw), "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    let sent = init();
    send(&mut stdin, sent.clone());
    let _ = read_response(&mut out);
    shutdown(child, stdin);

    let raw_lines = std::fs::read_to_string(&raw).unwrap_or_default();
    let received: Value =
        serde_json::from_str(raw_lines.lines().next().expect("upstream received a line")).unwrap();
    // The proxy added no transport-auth envelope: the upstream got exactly what the client sent.
    assert_eq!(
        received, sent,
        "proxy must forward verbatim, injecting nothing"
    );
    let text = raw_lines.to_lowercase();
    assert!(
        !text.contains("authorization") && !text.contains("\"token\""),
        "proxy must not inject an auth/token field"
    );
}

#[test]
fn initialize_and_tools_list_forward_and_response_is_unmutated() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let r = read_response(&mut out);
    // The upstream's canned serverInfo is relayed unmutated.
    assert_eq!(r["result"]["serverInfo"]["version"], "0.0.0");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["result"]["tools"][0]["name"], "echo");

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(methods.contains(&"initialize".to_string()));
    assert!(methods.contains(&"tools/list".to_string()));
}

#[test]
fn ping_is_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 5, "method": "ping"}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["id"], 5);
    assert!(r["result"].is_object());

    shutdown(child, stdin);
    assert!(read_methods(&log).contains(&"ping".to_string()));
}

#[test]
fn upstream_spawn_failure_yields_proxy_failed() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    // An upstream command that cannot be spawned.
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy",
            "--upstream-command",
            "this-binary-does-not-exist-assay-test",
        ])
        .env("MOCK_UPSTREAM_LOG", &log)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_FAILED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    assert_eq!(r["error"]["data"]["reason"], "upstream_spawn_failed");

    shutdown(child, stdin);
}

#[test]
fn malformed_upstream_response_is_not_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "malformed");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out); // initialize is normal

    // tools/list: the mock emits a non-JSON line; the proxy must surface a proxy_failed, never relay
    // the garbage as a successful result.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_FAILED);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    assert_eq!(r["error"]["data"]["reason"], "malformed_upstream_response");

    shutdown(child, stdin);
}

#[test]
fn default_mode_spawns_no_upstream_and_serves_local_tools() {
    // No `proxy` subcommand: the terminating server is unchanged and must not spawn an upstream. We
    // point at a sentinel log that must stay absent/empty because no mock can have been spawned.
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("must_stay_absent.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", "../../tests/fixtures/mcp"])
        .env("MOCK_UPSTREAM_LOG", &sentinel)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let r = read_response(&mut out);
    // The default server answers initialize itself (its own serverInfo, not the mock's).
    assert_ne!(r["result"]["serverInfo"]["name"], "mock-upstream");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let r = read_response(&mut out);
    assert!(
        r["result"]["tools"].is_array(),
        "default server serves its own tools"
    );

    shutdown(child, stdin);
    assert!(
        !sentinel.exists(),
        "default mode must not spawn any upstream (mock log must not exist)"
    );
}

#[test]
fn no_artifact_is_written_without_the_out_flag() {
    // The manifest-observation flag exists (P61c) but is opt-in: with no --mcp-manifest-observed-out,
    // the proxy writes no artifact. (Manifest emission itself is covered in proxy_manifest_e2e.rs.)
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let mut child = spawn_proxy(&log, None, "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    shutdown(child, stdin);

    let stray: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("manifest") || n.contains("observed"))
        .collect();
    assert!(
        stray.is_empty(),
        "no artifact without the out flag: {stray:?}"
    );
}
