//! P61e-c3: the enforcing-proxy PDP end to end — caller-allowance + credential-scope + drift gates,
//! and the first allow/forward path. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Deny-first: the deny matrix is asserted first, and across all of it no `tools/call` ever reaches the
//! upstream. The ONE happy path (full policy + approved baseline + a current complete observation whose
//! per-tool digest matches) is the only case that forwards. `--declared-mcp-manifest` is required in
//! enforcing mode; a missing/invalid policy OR baseline fails startup, never a runtime deny.

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

/// The committed P60a per-tool baseline. Its `github.add_deploy_key` tool_digest equals what the mock's
/// `p60a` mode produces (the same canonical tools), so it is the approved baseline for the happy path.
fn approved_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json")
}

/// A policy that allows github_deploy_key on acme/prod-app with a credential that exactly covers the
/// required scope.
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

const ALLOW_ACME_NO_CRED: &str = r#"
caller:
  id: "ci-agent"
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write");
    p
}

/// Spawn the enforcing proxy with a policy + baseline and the given mock mode.
fn spawn_enforce(log: &Path, policy: &Path, baseline: &Path, mode: &str) -> Child {
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
            baseline.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

/// Spawn the enforcing proxy that also writes the per-call enforcement-decision NDJSON (P61e-d).
fn spawn_enforce_with_decisions(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
) -> Child {
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
            baseline.to_str().unwrap(),
            "--enforcement-decision-out",
            decisions.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
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

/// init -> tools/call (NO tools/list first), with the approved baseline. Returns the deny reason and
/// the upstream method log. Asserts proxy_denied and that tools/call never reached the upstream.
fn deny_reason_for(policy_yaml: &str, call_params: Value) -> (String, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", policy_yaml);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "normal");
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
        "every deny outcome is proxy_denied; got {r}"
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
        "INVARIANT: a denied tools/call must never reach the upstream: {methods:?}"
    );
    (reason, methods)
}

// --- pre-drift gates (classification / allowance / credential), no observation needed --------------

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
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "staging-app"}}),
    );
    assert_eq!(reason, "no_declared_allowance");
}

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
    let (reason, _) = deny_reason_for(
        ALLOW_ACME_NO_CRED,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "credential_scope_unknown");
}

// --- drift gate (runs after the pre-drift gates pass) ---------------------------------------------

#[test]
fn matching_gates_without_observation_deny_current_observation_incomplete() {
    // Allowance + credential pass, but no tools/list was observed this session, so there is no current
    // complete manifest to compare -> fail closed (NOT an allow, and NOT pdp_gate_unavailable, gone).
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "manifest_current_observation_incomplete");
}

#[test]
fn tool_absent_from_baseline_denies_baseline_missing() {
    // A valid baseline that does not contain the invoked tool -> this tool has no approved baseline.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let baseline = write_file(
        dir.path(),
        "baseline.json",
        r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"search","tool_digest":"sha256:abc"}]}"#,
    );
    let mut child = spawn_enforce(&log, &policy, &baseline, "p60a");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "github.add_deploy_key",
                                      "arguments": {"owner": "acme", "repo": "prod-app"}}}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);
    assert_eq!(r["error"]["data"]["reason"], "manifest_baseline_missing");
    shutdown(child, stdin);
    assert!(!read_methods(&log).contains(&"tools/call".to_string()));
}

#[test]
fn observed_digest_differs_from_baseline_denies_drifted() {
    // A baseline whose github.add_deploy_key digest differs from what the p60a mock advertises -> drift.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let baseline = write_file(
        dir.path(),
        "baseline.json",
        r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"github.add_deploy_key","tool_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}]}"#,
    );
    let mut child = spawn_enforce(&log, &policy, &baseline, "p60a");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "github.add_deploy_key",
                                      "arguments": {"owner": "acme", "repo": "prod-app"}}}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_drifted_since_approval"
    );
    shutdown(child, stdin);
    assert!(!read_methods(&log).contains(&"tools/call".to_string()));
}

// --- the ONE happy path: every gate passes -> forward ---------------------------------------------

#[test]
fn fully_allowed_call_is_forwarded_and_response_relayed() {
    // Approved baseline + a current complete observation whose github.add_deploy_key digest matches +
    // matching allowance + covering credential -> the single allow path forwards, and the upstream's
    // reply relays back verbatim.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "p60a");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    // Observe a complete tools/list so the drift gate has a current digest for the tool.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let lst = read_response(&mut out);
    assert!(lst["result"]["tools"].is_array(), "tools/list relayed");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "github.add_deploy_key",
                                      "arguments": {"owner": "acme", "repo": "prod-app"}}}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["id"], 3);
    assert!(
        r.get("error").is_none(),
        "an allowed call is forwarded, not denied; got {r}"
    );
    assert_eq!(
        r["result"]["content"][0]["text"], "forwarded-ok",
        "the upstream's reply is relayed verbatim"
    );

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"tools/call".to_string()),
        "the allowed tools/call must reach the upstream: {methods:?}"
    );
}

// --- P61e-d: per-call enforcement_decision.v0 records ---------------------------------------------

#[test]
fn enforcement_decision_records_are_written_for_deny_and_allow() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child =
        spawn_enforce_with_decisions(&log, &policy, &approved_baseline_path(), "p60a", &decisions);
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    // Observe a complete manifest so the allowed call can clear the drift gate.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    // 1) unclassified -> deny.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let _ = read_response(&mut out);
    // 2) fully allowed -> forward.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                           "params": {"name": "github.add_deploy_key",
                                      "arguments": {"owner": "acme", "repo": "prod-app"}}}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");

    shutdown(child, stdin);

    let body = std::fs::read_to_string(&decisions).expect("decisions file written");
    let recs: Vec<Value> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is a JSON record"))
        .collect();
    assert_eq!(recs.len(), 2, "one record per tools/call decision: {body}");

    assert_eq!(recs[0]["schema"], "assay.enforcement_decision.v0");
    assert_eq!(recs[0]["decision"], "deny");
    assert_eq!(recs[0]["reason"], "unclassified_tool_call");
    assert_eq!(recs[0]["forwarded"], false);

    assert_eq!(recs[1]["decision"], "allow");
    assert_eq!(recs[1]["forwarded"], true);
    assert_eq!(recs[1]["drift_state"], "satisfied");
    assert_eq!(recs[1]["tool"]["action_class"], "github_deploy_key");
    assert_eq!(recs[1]["credential_alias"], "gh-deploy");

    // The declared credential scopes never appear in the evidence stream (alias only).
    assert!(
        !body.contains("repo:deploy_key:write"),
        "declared scopes must not leak into the decision records"
    );
}

// --- startup failures (non-zero exit; both inputs required in enforcing mode) ---------------------

fn startup_status(args: &[&str]) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn")
}

/// The fixed upstream-command args shared by the startup-failure cases.
fn upstream_args() -> Vec<String> {
    vec![
        "proxy-enforce".into(),
        "--upstream-command".into(),
        python().into(),
        "--upstream-arg".into(),
        "-u".into(),
        "--upstream-arg".into(),
        mock_path().to_str().unwrap().into(),
    ]
}

fn run_startup(extra: &[&str]) -> std::process::ExitStatus {
    let mut args = upstream_args();
    for e in extra {
        args.push((*e).into());
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    startup_status(&arg_refs)
}

#[test]
fn missing_enforce_policy_flag_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = approved_baseline_path();
    let status = run_startup(&["--declared-mcp-manifest", baseline.to_str().unwrap()]);
    let _ = dir;
    assert!(
        !status.success(),
        "missing --enforce-policy must fail startup"
    );
}

#[test]
fn missing_declared_manifest_flag_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let status = run_startup(&["--enforce-policy", policy.to_str().unwrap()]);
    assert!(
        !status.success(),
        "missing --declared-mcp-manifest must fail startup in enforcing mode"
    );
}

#[test]
fn missing_policy_file_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.yaml");
    let status = run_startup(&[
        "--enforce-policy",
        missing.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
    ]);
    assert!(!status.success(), "unreadable policy must fail startup");
}

#[test]
fn missing_caller_id_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", "allowances: []\n");
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "policy without caller.id must fail startup"
    );
}

#[test]
fn missing_declared_manifest_file_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let missing = dir.path().join("nope.json");
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        missing.to_str().unwrap(),
    ]);
    assert!(!status.success(), "unreadable baseline must fail startup");
}

#[test]
fn wrong_schema_declared_manifest_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let baseline = write_file(
        dir.path(),
        "baseline.json",
        r#"{"schema":"assay.mcp_manifest_observed.v0","tools":[{"name":"t","tool_digest":"sha256:abc"}]}"#,
    );
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        baseline.to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "a wrong-schema baseline must fail startup"
    );
}
