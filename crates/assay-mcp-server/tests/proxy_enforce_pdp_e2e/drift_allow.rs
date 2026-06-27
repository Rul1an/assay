use serde_json::Value;
use std::io::BufReader;

use crate::support::*;

// --- drift gate (runs after the pre-drift gates pass) ---------------------------------------------

#[test]
fn matching_gates_without_observation_deny_current_observation_incomplete() {
    // Allowance + credential pass, but no tools/list was observed this session, so there is no current
    // complete manifest to compare -> fail closed.
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "manifest_current_observation_incomplete");
}

#[test]
fn tool_absent_from_baseline_denies_baseline_missing() {
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
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "p60a");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
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
    assert!(r.get("error").is_none(), "allowed call was denied: {r}");
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");

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
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "echo", "arguments": {}}}),
    );
    let _ = read_response(&mut out);
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

    assert!(
        body.lines().all(|l| !l.contains("\"forwarded\"")),
        "decision records must not claim transport delivery: {body}"
    );
    assert!(
        read_methods(&log).contains(&"tools/call".to_string()),
        "the allowed call really reached the upstream"
    );
    assert!(
        !body.contains("repo:deploy_key:write"),
        "declared scopes must not leak into the decision records"
    );
}
