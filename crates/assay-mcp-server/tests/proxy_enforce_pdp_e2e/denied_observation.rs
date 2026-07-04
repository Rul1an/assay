use serde_json::Value;
use std::io::BufReader;
use std::process::{Command, Stdio};

use crate::support::*;

fn spawn_enforce_with_denied_observations(
    log: &std::path::Path,
    policy: &std::path::Path,
    denied_observations: &std::path::Path,
) -> std::process::Child {
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
            approved_baseline_path().to_str().unwrap(),
            "--denied-call-observation-out",
            denied_observations.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy")
}

fn read_denied_observation_records(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("denied observation record JSON"))
        .collect()
}

#[test]
fn denied_call_observation_records_caller_visible_proxy_denial_without_becoming_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let denied_observations = dir.path().join("denied_observations.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce_with_denied_observations(&log, &policy, &denied_observations);
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "tools/call",
            "params": {
                "name": "github.add_deploy_key",
                "arguments": {"owner": "acme", "repo": "prod-app"}
            }
        }),
    );
    let response = read_response(&mut out);
    assert_eq!(response["error"]["code"], PROXY_DENIED);
    assert_eq!(response["error"]["data"]["origin"], "assay-proxy");
    assert_eq!(
        response["error"]["data"]["reason"],
        "manifest_current_observation_incomplete"
    );
    shutdown(child, stdin);

    let records = read_denied_observation_records(&denied_observations);
    assert_eq!(
        records.len(),
        1,
        "one denied observation per answered denial"
    );
    let rec = &records[0];
    assert_eq!(rec["schema"], "assay.denied_call_observation.v0");
    assert_eq!(rec["call"]["tool_name"], "github.add_deploy_key");
    assert!(
        rec["call"]["target_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "record must bind to the classified call target: {rec}"
    );
    assert_eq!(rec["caller_visible_error"]["code"], PROXY_DENIED);
    assert_eq!(rec["caller_visible_error"]["origin"], "assay-proxy");
    assert_eq!(
        rec["caller_visible_error"]["reason"],
        "manifest_current_observation_incomplete"
    );
    assert!(
        rec["caller_visible_response_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "record must retain a digest of the exact caller-visible response: {rec}"
    );
    assert!(
        rec.get("decision").is_none(),
        "observation carrier must not become a verdict carrier"
    );
    assert!(rec["non_claims"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim.as_str().unwrap().contains("policy decision")));
}

#[test]
fn denied_call_observation_flag_off_writes_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let denied_observations = dir.path().join("denied_observations.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/call",
            "params": {
                "name": "github.add_deploy_key",
                "arguments": {"owner": "acme", "repo": "prod-app"}
            }
        }),
    );
    let response = read_response(&mut out);
    assert_eq!(response["error"]["code"], PROXY_DENIED);
    shutdown(child, stdin);

    assert!(
        !denied_observations.exists(),
        "denied observation file is opt-in"
    );
}
