use std::io::BufReader;
use std::process::{Command, Stdio};

use crate::support::*;

// =================================================================================================
// Increment 2b/2c: pre-call manifest-establish wiring and carrier emission.
// =================================================================================================

#[test]
fn establish_then_allow_forwards_without_a_client_list() {
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
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(methods.contains(&"tools/list".to_string()));
    assert!(methods.contains(&"tools/call".to_string()));

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

    let recs = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(
        recs.contains("assay.enforcement_decision.v0") && recs.contains("allow"),
        "the effective allow decision must be recorded before forwarding; recs={recs}"
    );
}

#[test]
fn establish_timeout_denies_to_client_within_budget() {
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
    assert_eq!(r["error"]["code"], PROXY_DENIED);
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
    assert!(methods.contains(&"tools/list".to_string()));
    assert!(!methods.contains(&"tools/call".to_string()));
}

#[test]
fn ambiguous_observation_denies_without_attempting_establish() {
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
    assert_eq!(lists, 1);
}

#[test]
fn establish_completes_but_tool_absent_denies() {
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
    assert!(methods.contains(&"tools/list".to_string()));
    assert!(!methods.contains(&"tools/call".to_string()));
}

#[test]
fn no_establish_needed_allow_writes_carrier_and_decision() {
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
    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec["establish_path"], "no_establish_needed");
    assert_eq!(rec["run_outcome"], "not_performed");
    assert_eq!(rec["establish_attempted"], serde_json::json!(false));
    assert_no_secrets(rec);
    let dec = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(dec.contains("assay.enforcement_decision.v0") && dec.contains("allow"));
}

#[test]
fn establish_then_allow_writes_established_then_allowed() {
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
    assert_eq!(lists, 1);
}
