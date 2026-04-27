use assay_evidence::{BundleWriter, EvidenceEvent};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        type_,
        "urn:assay:test:trust-basis-cli",
        run_id,
        seq,
        payload,
    );
    event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    event
}

fn write_bundle(path: &std::path::Path, events: Vec<EvidenceEvent>) {
    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().unwrap();
}

fn claim<'a>(claims: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    claims
        .iter()
        .find(|claim| claim["id"] == id)
        .expect("claim should exist")
}

#[test]
fn trust_basis_generate_stdout_emits_all_frozen_claims() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("trust-basis.tar.gz");
    write_bundle(
        &bundle,
        vec![
            make_event(
                "assay.tool.decision",
                "run_stdout",
                0,
                json!({
                    "tool": "tool.commit",
                    "decision": "allow",
                    "delegated_from": "agent:planner"
                }),
            ),
            make_event(
                "assay.sandbox.degraded",
                "run_stdout",
                1,
                json!({
                    "reason_code": "policy_conflict",
                    "degradation_mode": "audit_fallback",
                    "component": "landlock"
                }),
            ),
        ],
    );

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(
        claims.len(),
        8,
        "all frozen claims should always be present"
    );

    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "delegation_context_visible")["level"],
        "verified"
    );
    assert_eq!(
        claim(claims, "authorization_context_visible")["level"],
        "absent"
    );
    assert_eq!(
        claim(claims, "containment_degradation_observed")["level"],
        "verified"
    );
    assert_eq!(claim(claims, "signing_evidence_present")["level"], "absent");
    assert_eq!(
        claim(claims, "provenance_backed_claims_present")["level"],
        "absent"
    );
    assert_eq!(
        claim(claims, "applied_pack_findings_present")["level"],
        "absent"
    );
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent"
    );
}

#[test]
fn trust_basis_generate_is_byte_stable_and_pack_aware() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("trust-basis-pack.tar.gz");
    write_bundle(
        &bundle,
        vec![make_event(
            "assay.tool.decision",
            "run_pack",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "principal": "user:alice"
            }),
        )],
    );

    let run = || {
        Command::cargo_bin("assay")
            .unwrap()
            .arg("trust-basis")
            .arg("generate")
            .arg(&bundle)
            .arg("--pack")
            .arg("owasp-agentic-a3-a5-signal-followup")
            .output()
            .unwrap()
    };

    let first = run();
    let second = run();
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(
        first.stdout, second.stdout,
        "canonical trust basis should regenerate byte-for-byte"
    );

    let json: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(
        claim(claims, "applied_pack_findings_present")["level"],
        "verified"
    );
    assert_eq!(
        claim(claims, "applied_pack_findings_present")["source"],
        "pack_execution_results"
    );
    assert_eq!(
        claim(claims, "applied_pack_findings_present")["boundary"],
        "pack-execution-only"
    );
}
