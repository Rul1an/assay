//! P61e-c1: the enforcing-proxy caller-allowance PDP, end to end through the spawned binary.
//! Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! These tests exercise the gate matrix with the real classifier and a real policy file. They are
//! deliberately negative-first: c1 has NO allow path, so the strongest assertion is that no `tools/call`
//! ever reaches the upstream regardless of the deny reason. A call that passes every c1 gate is denied
//! with `pdp_gate_unavailable` (the temporary rollout reason removed when c3 lands). Startup failures
//! (missing/malformed policy, missing caller.id) are asserted as a non-zero exit, never a runtime deny.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

/// A policy that allows github_deploy_key on exactly acme/prod-app, with a credential whose scope
/// exactly covers the required scope (so a matching call clears the c2 credential-scope gate too).
const ALLOW_ACME: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-deploy"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

/// Same allowance, but the declared credential does NOT cover the required scope.
const ALLOW_ACME_INSUFFICIENT_CRED: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-ro"
  scopes: ["repo:read"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

/// Same allowance, but no credential is declared at all (coverage cannot be determined).
const ALLOW_ACME_NO_CRED: &str = r#"
caller:
  id: "ci-agent"
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

fn write_policy(dir: &Path, yaml: &str) -> PathBuf {
    let p = dir.join("enforce.yaml");
    std::fs::write(&p, yaml).expect("write policy");
    p
}

fn spawn_enforce(log: &Path, policy: &Path) -> Child {
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
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

fn send(stdin: &mut ChildStdin, v: Value) {
    writeln!(stdin, "{v}").expect("write");
    stdin.flush().expect("flush");
}

fn read_response(reader: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("read");
        assert!(n > 0, "proxy closed stdout before responding");
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(t).expect("parse JSON");
        if v.get("method").is_some() {
            continue;
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

fn read_methods(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

fn shutdown(mut child: Child, stdin: ChildStdin) {
    drop(stdin);
    let _ = child.wait();
}

/// Drive one tools/call through a freshly spawned enforcing proxy and return the deny reason plus the
/// upstream method log. Always asserts proxy_denied (c1 has no allow path).
fn deny_reason_for(policy_yaml: &str, call_params: Value) -> (String, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_policy(dir.path(), policy_yaml);
    let mut child = spawn_enforce(&log, &policy);
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": call_params}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["id"], 9);
    assert_eq!(
        r["error"]["code"], PROXY_DENIED,
        "every c1 outcome is a proxy_denied; got {r}"
    );
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    let reason = r["error"]["data"]["reason"]
        .as_str()
        .expect("reason string")
        .to_string();

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "INVARIANT: tools/call must never reach the upstream: {methods:?}"
    );
    (reason, methods)
}

// --- gate matrix (deny-only) --------------------------------------------------------------------

#[test]
fn unclassified_tool_call_denied_unclassified() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "echo", "arguments": {}}),
    );
    assert_eq!(reason, "unclassified_tool_call");
}

#[test]
fn classification_incomplete_denied_before_allowance() {
    // Missing repo -> classified_incomplete. Must read as classification_incomplete, NOT a target
    // mismatch — the classification gate runs before allowance matching.
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key", "arguments": {"owner": "acme"}}),
    );
    assert_eq!(reason, "classification_incomplete");
}

#[test]
fn classified_privileged_without_matching_allowance_denied() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "evil", "repo": "x"}}),
    );
    assert_eq!(reason, "no_declared_allowance");
}

#[test]
fn allowance_target_mismatch_denied_no_declared_allowance() {
    // Right owner, wrong repo: a target mismatch is no_declared_allowance, not a partial match.
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "staging-app"}}),
    );
    assert_eq!(reason, "no_declared_allowance");
}

#[test]
fn matching_allowance_and_covering_scope_reaches_pdp_gate_unavailable() {
    // The one path that clears every enabled gate (allowance + credential-scope) is still denied:
    // there is no allow/forward path before c3.
    let (reason, methods) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "pdp_gate_unavailable");
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "even a fully-allowed call does not forward before c3"
    );
}

// --- c2 credential-scope gate (runs after the allowance matches) --------------------------------

#[test]
fn insufficient_credential_scope_denied() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME_INSUFFICIENT_CRED,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "credential_scope_insufficient");
}

#[test]
fn no_declared_credential_is_scope_unknown() {
    // Coverage cannot be determined -> unknown, never a silent pass and never "insufficient".
    let (reason, _) = deny_reason_for(
        ALLOW_ACME_NO_CRED,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "credential_scope_unknown");
}

// --- startup failures (non-zero exit, never a runtime deny) -------------------------------------

fn startup_status(args: &[&str]) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn")
}

#[test]
fn missing_enforce_policy_flag_fails_startup() {
    // No --enforce-policy at all: clap rejects it as a usage error (non-zero), never starts the proxy.
    let status = startup_status(&[
        "proxy-enforce",
        "--upstream-command",
        python(),
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        mock_path().to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "missing --enforce-policy must fail startup"
    );
}

#[test]
fn missing_policy_file_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.yaml");
    let status = startup_status(&[
        "proxy-enforce",
        "--upstream-command",
        python(),
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        mock_path().to_str().unwrap(),
        "--enforce-policy",
        missing.to_str().unwrap(),
    ]);
    assert!(!status.success(), "unreadable policy must fail startup");
}

#[test]
fn malformed_policy_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_policy(dir.path(), "caller: : :\n");
    let status = startup_status(&[
        "proxy-enforce",
        "--upstream-command",
        python(),
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        mock_path().to_str().unwrap(),
        "--enforce-policy",
        p.to_str().unwrap(),
    ]);
    assert!(!status.success(), "malformed policy must fail startup");
}

#[test]
fn missing_caller_id_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_policy(dir.path(), "allowances: []\n");
    let status = startup_status(&[
        "proxy-enforce",
        "--upstream-command",
        python(),
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        mock_path().to_str().unwrap(),
        "--enforce-policy",
        p.to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "policy without caller.id must fail startup"
    );
}
