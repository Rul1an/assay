//! P61e-c3: the enforcing-proxy PDP end to end — caller-allowance + credential-scope + drift gates,
//! and the first allow/forward path. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Deny-first: the deny matrix is asserted first, and across all of it no `tools/call` ever reaches the
//! upstream. The ONE happy path (full policy + approved baseline + a current complete observation whose
//! per-tool digest matches) is the only case that forwards. `--declared-mcp-manifest` is required in
//! enforcing mode; a missing/invalid policy OR baseline fails startup, never a runtime deny.

use assay_mcp_server::manifest_observed::{build_observed, Completeness};
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

    assert_eq!(recs[1]["decision"], "allow");
    assert_eq!(recs[1]["drift_state"], "satisfied");
    assert_eq!(recs[1]["tool"]["action_class"], "github_deploy_key");
    assert_eq!(recs[1]["credential_alias"], "gh-deploy");

    // The record carries no transport-outcome field — an allow is the policy decision, not a delivery
    // claim. The actual delivery is proven separately: only the allowed call reaches the upstream.
    assert!(
        body.lines().all(|l| !l.contains("\"forwarded\"")),
        "decision records must not claim transport delivery: {body}"
    );
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"tools/call".to_string()),
        "the allowed call really reached the upstream: {methods:?}"
    );

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

// =================================================================================================
// Increment 2b: pre-call manifest-establish wiring (establish -> re-decide -> forward|deny).
// =================================================================================================

const DEPLOY_KEY_CALL_ID: i64 = 9;

fn deploy_key_call() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": DEPLOY_KEY_CALL_ID, "method": "tools/call",
        "params": {"name": "github.add_deploy_key", "arguments": {"owner": "acme", "repo": "prod-app"}}
    })
}

/// Read every remaining stdout line to EOF (used to prove nothing leaked to the client).
fn drain_stdout(reader: &mut BufReader<ChildStdout>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let t = buf.trim();
                if !t.is_empty() {
                    lines.push(t.to_string());
                }
            }
        }
    }
    lines
}

#[test]
fn establish_then_allow_forwards_without_a_client_list() {
    // init -> tools/call with NO client tools/list: observed is NoCompleteManifest, so the proxy runs a
    // pre-call establish (proxy-originated tools/list). The p60a mock answers a complete manifest whose
    // github.add_deploy_key digest matches the approved baseline, the re-decided call is ALLOWED and
    // forwarded, and the EFFECTIVE allow is recorded. The first real establish-derived allow path.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child =
        spawn_enforce_with_decisions(&log, &policy, &approved_baseline_path(), "p60a", &decisions);
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let init_resp = read_response(&mut out);
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "mock-upstream");

    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["id"], DEPLOY_KEY_CALL_ID);
    assert_eq!(
        r["result"]["content"][0]["text"], "forwarded-ok",
        "establish completes the manifest and the re-decided call is forwarded; got {r}"
    );

    shutdown(child, stdin);

    // The upstream saw the synthetic establish tools/list AND the forwarded tools/call.
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"tools/list".to_string()),
        "establish must originate a tools/list; methods={methods:?}"
    );
    assert!(
        methods.contains(&"tools/call".to_string()),
        "the allowed call must be forwarded; methods={methods:?}"
    );

    // Non-leakage (end-to-end): no client-visible line carries a reserved establish id, and nothing
    // leaks beyond the two responses to the client's own requests.
    assert!(!serde_json::to_string(&init_resp)
        .unwrap()
        .contains("assay-establish-"));
    assert!(!serde_json::to_string(&r)
        .unwrap()
        .contains("assay-establish-"));
    for line in drain_stdout(&mut out) {
        assert!(
            !line.contains("assay-establish-"),
            "a synthetic establish line leaked to the client: {line}"
        );
    }

    // Record-before-forward: the EFFECTIVE (allow) decision was written to enforcement_decision.v0.
    let recs = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(
        recs.contains("assay.enforcement_decision.v0") && recs.contains("allow"),
        "the effective allow decision must be recorded before forwarding; recs={recs}"
    );
}

#[test]
fn establish_timeout_denies_to_client_within_budget() {
    // The mock receives the establish tools/list but never answers it. The establish must time out
    // within its (test-shrunk) budget and return a deny to the ORIGINAL tools/call -- the client is
    // never left hanging behind a blocked establish.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
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
            approved_baseline_path().to_str().unwrap(),
            "--manifest-establish-budget-ms",
            "300",
        ])
        .env("MOCK_UPSTREAM_LOG", &log)
        .env("MOCK_UPSTREAM_MODE", "drop_list")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    let start = std::time::Instant::now();
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    let elapsed = start.elapsed();

    assert_eq!(r["id"], DEPLOY_KEY_CALL_ID);
    assert_eq!(
        r["error"]["code"], PROXY_DENIED,
        "establish timeout -> deny; got {r}"
    );
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_current_observation_incomplete"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "the client must get its deny within budget + margin, not hang; took {elapsed:?}"
    );

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"tools/list".to_string()),
        "establish was attempted (synthetic list sent); methods={methods:?}"
    );
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "a timed-out establish never forwards the call; methods={methods:?}"
    );
}

#[test]
fn ambiguous_observation_denies_without_attempting_establish() {
    // The client observes a duplicate-name (ambiguous) tools/list, then calls. Ambiguity cannot be
    // resolved by re-listing, so the proxy denies WITHOUT originating an establish tools/list.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "duplicate");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_observation_ambiguous"
    );

    shutdown(child, stdin);
    let lists = read_methods(&log)
        .iter()
        .filter(|m| m.as_str() == "tools/list")
        .count();
    assert_eq!(
        lists, 1,
        "ambiguous must NOT trigger an establish re-list; only the client's list should appear"
    );
}

#[test]
fn establish_completes_but_tool_absent_denies() {
    // normal mode returns a complete manifest that lacks github.add_deploy_key. init -> tools/call with
    // no client list: establish originates a list, gets a complete manifest WITHOUT the tool
    // (CompleteButToolAbsent) -> re-decide deny. The synthetic list was attempted; the call is not
    // forwarded.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_current_observation_incomplete"
    );

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(
        methods.contains(&"tools/list".to_string()),
        "establish was attempted; methods={methods:?}"
    );
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "an absent tool is never forwarded; methods={methods:?}"
    );
}

// =================================================================================================
// Increment 2c: assay.manifest_establish.v0 carrier emission + operator budget flag.
// =================================================================================================

/// Spawn the enforcing proxy writing BOTH the enforcement-decision NDJSON and the manifest-establish
/// carrier NDJSON, with an optional establish budget (ms).
fn spawn_enforce_recording(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
    establish: &Path,
    budget_ms: Option<u64>,
) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.arg("proxy-enforce")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u"])
        .args(["--upstream-arg", mock_path().to_str().unwrap()])
        .args(["--enforce-policy", policy.to_str().unwrap()])
        .args(["--declared-mcp-manifest", baseline.to_str().unwrap()])
        .args(["--enforcement-decision-out", decisions.to_str().unwrap()])
        .args(["--manifest-establish-out", establish.to_str().unwrap()]);
    if let Some(ms) = budget_ms {
        cmd.args(["--manifest-establish-budget-ms", &ms.to_string()]);
    }
    cmd.env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

fn read_establish_records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("establish record JSON"))
        .collect()
}

/// The carrier carries no raw scope/target/token/credential/caller/args — journey only.
fn assert_no_secrets(rec: &Value) {
    let s = serde_json::to_string(rec).unwrap();
    for forbidden in [
        "target_digest",
        "scope",
        "token",
        "credential",
        "caller",
        "arguments",
        "owner",
        "repo",
    ] {
        assert!(
            !s.contains(forbidden),
            "manifest_establish record must not carry `{forbidden}`: {s}"
        );
    }
    // Exactly the five v0 fields, nothing more.
    let obj = rec.as_object().expect("record is an object");
    let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "action_class",
            "establish_attempted",
            "establish_path",
            "run_outcome",
            "schema"
        ]
    );
}

#[test]
fn no_establish_needed_allow_writes_carrier_and_decision() {
    // A client tools/list establishes a current complete manifest the old way; the call is allowed with
    // NO establish run -> carrier is no_establish_needed / not_performed / not attempted, alongside an
    // allowed enforcement decision (the two carriers are orthogonal).
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let establish = dir.path().join("establish.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_recording(
        &log,
        &policy,
        &approved_baseline_path(),
        "p60a",
        &decisions,
        &establish,
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");

    shutdown(child, stdin);
    let recs = read_establish_records(&establish);
    assert_eq!(recs.len(), 1, "one establish record per tools/call");
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "no_establish_needed");
    assert_eq!(rec["run_outcome"], "not_performed");
    assert_eq!(rec["establish_attempted"], serde_json::json!(false));
    assert_no_secrets(rec);
    // Orthogonal allowed decision in the separate verdict carrier.
    let dec = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(dec.contains("assay.enforcement_decision.v0") && dec.contains("allow"));
}

#[test]
fn establish_then_allow_writes_established_then_allowed() {
    // No client list -> establish runs, p60a completes a matching manifest -> allow + forward. Carrier
    // is established_then_allowed / complete / attempted.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let establish = dir.path().join("establish.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_recording(
        &log,
        &policy,
        &approved_baseline_path(),
        "p60a",
        &decisions,
        &establish,
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");

    shutdown(child, stdin);
    let recs = read_establish_records(&establish);
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "established_then_allowed");
    assert_eq!(rec["run_outcome"], "complete");
    assert_eq!(rec["establish_attempted"], serde_json::json!(true));
    assert_eq!(rec["action_class"], "github_deploy_key");
    assert_no_secrets(rec);
}

#[test]
fn establish_complete_but_absent_writes_established_then_denied() {
    // normal mode completes a manifest WITHOUT the tool -> CompleteButToolAbsent -> deny. The establish
    // ran and completed, so run_outcome is complete and the path is established_then_denied.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let establish = dir.path().join("establish.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_recording(
        &log,
        &policy,
        &approved_baseline_path(),
        "normal",
        &decisions,
        &establish,
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);

    shutdown(child, stdin);
    let recs = read_establish_records(&establish);
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "established_then_denied");
    assert_eq!(rec["run_outcome"], "complete");
    assert_eq!(rec["establish_attempted"], serde_json::json!(true));
    assert_no_secrets(rec);
}

#[test]
fn establish_timeout_writes_immediate_deny_timed_out() {
    // drop_list never answers the synthetic list -> establish times out within the budget -> deny. The
    // establish was attempted (run_outcome timed_out) but produced no completion -> immediate_deny.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let establish = dir.path().join("establish.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_recording(
        &log,
        &policy,
        &approved_baseline_path(),
        "drop_list",
        &decisions,
        &establish,
        Some(300),
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["code"], PROXY_DENIED);

    shutdown(child, stdin);
    let recs = read_establish_records(&establish);
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "immediate_deny");
    assert_eq!(rec["run_outcome"], "timed_out");
    assert_eq!(rec["establish_attempted"], serde_json::json!(true));
    assert_no_secrets(rec);
}

#[test]
fn ambiguous_writes_immediate_deny_not_performed_no_synthetic_list() {
    // Ambiguous observation -> deny WITHOUT establish. Carrier is immediate_deny / not_performed / not
    // attempted, and the upstream saw only the client's tools/list (no synthetic establish list).
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let establish = dir.path().join("establish.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_recording(
        &log,
        &policy,
        &approved_baseline_path(),
        "duplicate",
        &decisions,
        &establish,
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_observation_ambiguous"
    );

    shutdown(child, stdin);
    let recs = read_establish_records(&establish);
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "immediate_deny");
    assert_eq!(rec["run_outcome"], "not_performed");
    assert_eq!(rec["establish_attempted"], serde_json::json!(false));
    assert_no_secrets(rec);
    let lists = read_methods(&log)
        .iter()
        .filter(|m| m.as_str() == "tools/list")
        .count();
    assert_eq!(
        lists, 1,
        "ambiguous must not originate a synthetic establish list"
    );
}

#[test]
fn establish_budget_zero_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
        "--manifest-establish-budget-ms",
        "0",
    ]);
    assert!(
        !status.success(),
        "a zero establish budget must be rejected at startup"
    );
}

// =================================================================================================
// Increment 5b: assay.tool_annotation_conformance.v0 carrier emission (relay wiring).
// =================================================================================================

/// Spawn the enforcing proxy writing the enforcement-decision NDJSON and, optionally, the
/// tool-annotation conformance carrier NDJSON, with an optional establish budget (ms).
fn spawn_enforce_conformance(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
    conformance: Option<&Path>,
    budget_ms: Option<u64>,
) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.arg("proxy-enforce")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u"])
        .args(["--upstream-arg", mock_path().to_str().unwrap()])
        .args(["--enforce-policy", policy.to_str().unwrap()])
        .args(["--declared-mcp-manifest", baseline.to_str().unwrap()])
        .args(["--enforcement-decision-out", decisions.to_str().unwrap()]);
    if let Some(c) = conformance {
        cmd.args(["--tool-conformance-out", c.to_str().unwrap()]);
    }
    if let Some(ms) = budget_ms {
        cmd.args(["--manifest-establish-budget-ms", &ms.to_string()]);
    }
    cmd.env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

fn read_conformance_records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("conformance record JSON"))
        .collect()
}

#[test]
fn complete_manifest_allow_emits_conformance_record() {
    // A client tools/list completes a matching manifest -> allow + forward. The conformance carrier
    // records observation_basis=complete with the real per-tool digest; the mock declares no
    // annotations, so conformance is `undeclared`, orthogonal to the allow verdict.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let conformance = dir.path().join("conformance.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_conformance(
        &log,
        &policy,
        &approved_baseline_path(),
        "p60a",
        &decisions,
        Some(&conformance),
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");
    shutdown(child, stdin);

    let recs = read_conformance_records(&conformance);
    assert_eq!(recs.len(), 1, "one conformance record per tools/call");
    let rec = &recs[0];
    assert_eq!(rec["schema"], "assay.tool_annotation_conformance.v0");
    assert_eq!(rec["observation_basis"], "complete");
    assert!(
        rec["tool"]["tool_digest"]
            .as_str()
            .is_some_and(|d| d.starts_with("sha256:")),
        "complete basis records the real per-tool digest: {rec}"
    );
    assert_eq!(rec["conformance"], "undeclared");
    // Orthogonal allow in the separate verdict carrier.
    let dec = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(dec.contains("assay.enforcement_decision.v0") && dec.contains("allow"));
}

#[test]
fn incomplete_manifest_emits_inconclusive_conformance() {
    // No client list + drop_list: the establish re-list times out, the effective observation is
    // never complete, so the call is denied AND the conformance carrier records
    // observation_basis=incomplete with a null digest and inconclusive conformance (annotations were
    // not observed, never a false `undeclared`).
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let conformance = dir.path().join("conformance.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_conformance(
        &log,
        &policy,
        &approved_baseline_path(),
        "drop_list",
        &decisions,
        Some(&conformance),
        Some(300),
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(
        r["error"]["data"]["reason"],
        "manifest_current_observation_incomplete"
    );
    shutdown(child, stdin);

    let recs = read_conformance_records(&conformance);
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["observation_basis"], "incomplete");
    assert_eq!(rec["conformance"], "inconclusive");
    assert_eq!(rec["tool"]["tool_digest"], Value::Null);
}

#[test]
fn conformance_flag_off_writes_no_file() {
    // Without --tool-conformance-out, no carrier file is produced.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let conformance = dir.path().join("conformance.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_conformance(
        &log,
        &policy,
        &approved_baseline_path(),
        "p60a",
        &decisions,
        None,
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let _ = read_response(&mut out);
    shutdown(child, stdin);

    assert!(
        !conformance.exists(),
        "no conformance file is written when the flag is off"
    );
}

#[test]
fn conformance_write_failure_on_allow_fails_closed() {
    // The conformance path is a directory, so the append fails. On an allow that must fail closed
    // (proxy_failed): an allow is the decision to forward, never a forwarded-but-unrecorded call.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let conformance_dir = dir.path().join("conformance_is_a_dir");
    std::fs::create_dir(&conformance_dir).unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_conformance(
        &log,
        &policy,
        &approved_baseline_path(),
        "p60a",
        &decisions,
        Some(&conformance_dir),
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    assert_eq!(
        r["error"]["data"]["reason"],
        "enforcement_record_write_failed"
    );
    shutdown(child, stdin);
    // The silent-unrecorded-forward regression this guards: a fail-closed allow must never reach
    // the upstream.
    let methods = read_methods(&log);
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "a fail-closed allow must never reach the upstream: {methods:?}"
    );
}

// =================================================================================================
// Increment 5b: the deferred live-regression on the conformance MISMATCH arm (PR #1672 follow-up).
//
// The unit/contract suite already exercises `conformance: mismatched`, but only over a hand-built
// declaration. This is the one end-to-end case 5b left out: a live tools/call through the enforcing
// proxy emits a `mismatched` conformance record WHILE the enforcement verdict is still `allow` — the
// core non-correlation property (a conformance mismatch is a signal, never a deny). For a `complete`
// observation basis the declared annotations must come from a fully observed manifest whose per-tool
// digest equals the approved baseline, so the mock declares `readOnlyHint:true` on
// github.add_deploy_key and the baseline below carries that SAME annotated digest.
// =================================================================================================

/// The approved baseline for the annotated mock mode. Its github.add_deploy_key tool_digest includes
/// `readOnlyHint:true` (generated by the real producer below), so the drift gate matches and allows.
fn readonly_annotation_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline_readonly_annotation.json",
    )
}

/// EXACTLY the raw tool definitions the mock's `p60a_readonly_annotation` mode emits: the canonical
/// P60a tools, but github.add_deploy_key declares `readOnlyHint:true`. The annotation rides into the
/// per-tool projection, so `build_observed` yields the same annotated tool_digest the live proxy
/// computes from the mock's `tools/list`.
fn annotated_p60a_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "search",
            "description": "does a thing",
            "inputSchema": {"type": "object"}
        }),
        serde_json::json!({
            "name": "github.add_deploy_key",
            "description": "Add a deploy key",
            "inputSchema": {"type": "object", "required": ["owner", "repo"]},
            "annotations": {"readOnlyHint": true}
        }),
    ]
}

/// The approved baseline generated from the REAL producer (`manifest_observed::build_observed`),
/// reshaped into a declared approval baseline. Generating it this way (never by hand) guarantees the
/// github.add_deploy_key tool_digest includes the annotation and equals what the proxy observes live.
fn annotated_baseline_doc() -> Value {
    let observed = build_observed("github", &annotated_p60a_tools(), Completeness::Complete);
    serde_json::json!({
        "note": "GENERATED from manifest_observed::build_observed over the annotated p60a tools (readOnlyHint:true on github.add_deploy_key); regenerate with ASSAY_UPDATE_GOLDEN=1. Pins the live conformance-mismatch e2e baseline.",
        "schema": "assay.declared_mcp_manifest.v0",
        "server": observed["server"],
        "canonicalization": observed["observed"]["canonicalization"],
        "manifest_digest": observed["observed"]["manifest_digest"],
        "tools": observed["observed"]["tool_digests"],
    })
}

fn deploy_key_digest(manifest: &Value) -> String {
    manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "github.add_deploy_key")
        .expect("github.add_deploy_key in baseline")["tool_digest"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn readonly_annotation_baseline_fixture_matches_producer() {
    // The committed baseline is REAL producer output, not a hand-authored digest. Regenerate after an
    // intentional producer change: ASSAY_UPDATE_GOLDEN=1 cargo test -p assay-mcp-server
    // --test proxy_enforce_pdp_e2e readonly_annotation_baseline_fixture_matches_producer.
    let generated = annotated_baseline_doc();
    let path = readonly_annotation_baseline_path();
    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&generated).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
    }
    let committed: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
                path.display()
            )
        }))
        .unwrap();
    assert_eq!(
        committed, generated,
        "the readonly-annotation baseline fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
    );

    // The load-bearing property: this digest is annotation-sensitive. If it equalled the un-annotated
    // p60a baseline, the e2e below would not actually exercise the declared-annotation path.
    let plain: Value =
        serde_json::from_str(&std::fs::read_to_string(approved_baseline_path()).unwrap()).unwrap();
    assert_ne!(
        deploy_key_digest(&committed),
        deploy_key_digest(&plain),
        "the annotated baseline digest must differ from the un-annotated p60a baseline"
    );
}

#[test]
fn live_conformance_mismatch_is_emitted_while_verdict_allows() {
    // The mock declares readOnlyHint:true on github.add_deploy_key while the call stays a create. The
    // approved baseline carries that SAME annotated digest, so every gate passes -> the verdict is allow
    // and the call forwards, yet the conformance carrier records a `mismatched` (declared read-only,
    // observed mutating) signal. Verdict and conformance are decided from one shared observation
    // snapshot, so this proves they are genuinely orthogonal, end to end.
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let decisions = dir.path().join("decisions.ndjson");
    let conformance = dir.path().join("conformance.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_conformance(
        &log,
        &policy,
        &readonly_annotation_baseline_path(),
        "p60a_readonly_annotation",
        &decisions,
        Some(&conformance),
        None,
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    // A client tools/list establishes a current complete manifest whose annotated github.add_deploy_key
    // digest matches the approved baseline.
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let lst = read_response(&mut out);
    assert!(lst["result"]["tools"].is_array(), "tools/list relayed");

    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    // Verdict ALLOW: the call is forwarded despite the read-only annotation, and the upstream reply
    // relays back verbatim.
    assert_eq!(r["id"], DEPLOY_KEY_CALL_ID);
    assert!(
        r.get("error").is_none(),
        "the call is allowed despite the read-only annotation; got {r}"
    );
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");
    shutdown(child, stdin);

    // The allowed call really reached the upstream.
    assert!(
        read_methods(&log).contains(&"tools/call".to_string()),
        "the allowed tools/call must reach the upstream"
    );

    // Conformance carrier: a complete-basis MISMATCH on the read-only axis, with the real per-tool
    // digest recorded.
    let recs = read_conformance_records(&conformance);
    assert_eq!(recs.len(), 1, "one conformance record per tools/call");
    let rec = &recs[0];
    assert_eq!(rec["schema"], "assay.tool_annotation_conformance.v0");
    assert_eq!(rec["observation_basis"], "complete");
    assert_eq!(rec["conformance"], "mismatched");
    assert_eq!(rec["mismatch_kind"], "declared_read_only_observed_mutating");
    assert_eq!(rec["declared"]["read_only"], serde_json::json!(true));
    assert_eq!(rec["observed"]["behavior_class"], "mutating");
    assert!(
        rec["tool"]["tool_digest"]
            .as_str()
            .is_some_and(|d| d.starts_with("sha256:")),
        "complete basis records the real per-tool digest: {rec}"
    );

    // The verdict carrier is orthogonal: the SAME call is an allow with the drift gate satisfied.
    let dec_body = std::fs::read_to_string(&decisions).unwrap_or_default();
    let decs: Vec<Value> = dec_body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is a JSON record"))
        .collect();
    assert_eq!(
        decs.len(),
        1,
        "exactly one enforcement decision for the single tools/call: {dec_body}"
    );
    assert_eq!(decs[0]["schema"], "assay.enforcement_decision.v0");
    assert_eq!(
        decs[0]["decision"], "allow",
        "the mismatched-annotation call is still allowed: {dec_body}"
    );
    assert_eq!(decs[0]["drift_state"], "satisfied");
    assert_eq!(decs[0]["tool"]["action_class"], "github_deploy_key");
}
