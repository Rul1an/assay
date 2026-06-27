use assay_mcp_server::manifest_observed::{build_observed, Completeness};
use serde_json::Value;
use std::io::BufReader;
use std::path::PathBuf;

use crate::support::*;

// =================================================================================================
// Increment 5b: assay.tool_annotation_conformance.v0 carrier emission (relay wiring).
// =================================================================================================

#[test]
fn complete_manifest_allow_emits_conformance_record() {
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
    let dec = std::fs::read_to_string(&decisions).unwrap_or_default();
    assert!(dec.contains("assay.enforcement_decision.v0") && dec.contains("allow"));
}

#[test]
fn incomplete_manifest_emits_inconclusive_conformance() {
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
    let methods = read_methods(&log);
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "a fail-closed allow must never reach the upstream: {methods:?}"
    );
}

/// The approved baseline for the annotated mock mode. Its github.add_deploy_key tool_digest includes
/// `readOnlyHint:true` (generated by the real producer below), so the drift gate matches and allows.
fn readonly_annotation_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline_readonly_annotation.json",
    )
}

/// EXACTLY the raw tool definitions the mock's `p60a_readonly_annotation` mode emits.
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
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let lst = read_response(&mut out);
    assert!(lst["result"]["tools"].is_array(), "tools/list relayed");

    send(&mut stdin, deploy_key_call());
    let r = read_response(&mut out);
    assert_eq!(r["id"], DEPLOY_KEY_CALL_ID);
    assert!(
        r.get("error").is_none(),
        "the call is allowed despite the read-only annotation; got {r}"
    );
    assert_eq!(r["result"]["content"][0]["text"], "forwarded-ok");
    shutdown(child, stdin);

    assert!(read_methods(&log).contains(&"tools/call".to_string()));

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

    let dec_body = std::fs::read_to_string(&decisions).unwrap_or_default();
    let decs: Vec<Value> = dec_body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is a JSON record"))
        .collect();
    assert_eq!(decs.len(), 1);
    assert_eq!(decs[0]["schema"], "assay.enforcement_decision.v0");
    assert_eq!(decs[0]["decision"], "allow");
    assert_eq!(decs[0]["drift_state"], "satisfied");
    assert_eq!(decs[0]["tool"]["action_class"], "github_deploy_key");
}
