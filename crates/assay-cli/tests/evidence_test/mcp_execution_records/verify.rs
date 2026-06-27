use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

use super::fixtures::{
    attestation_digest, attestation_json, binding_nonce, decision_json, decision_json_with_value,
    jcs_digest_json, outcome_json, outcome_json_with_backlink, request_envelope_json,
    substituted_binding_nonce,
};

#[test]
fn verify_mcp_records_reports_pairing_as_independent_consumer() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let attestation_digest = attestation_digest();
    let binding_nonce = binding_nonce();
    let decision_body = decision_json(&attestation_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(
        &outcome,
        outcome_json(&attestation_digest, &decision_digest),
    )
    .unwrap();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["verification_scope"]["role"], "independent-consumer");
    assert_eq!(report["binding"]["mode"], "sep2787_attestation");
    assert_eq!(
        report["binding"]["digest"].as_str(),
        Some(attestation_digest.as_str())
    );
    assert_eq!(
        report["binding"]["nonce"].as_str(),
        Some(binding_nonce.as_str())
    );
    assert_eq!(report["binding"]["nonce_source"], "issuerAsserted.nonce");
    assert_eq!(
        report["attestation"]["digest"].as_str(),
        Some(attestation_digest.as_str())
    );
    assert_eq!(report["decision"]["decision"], "allow");
    assert_eq!(report["outcome"]["status"], "executed");
    assert_eq!(report["outcome"]["decision_digest"], decision_digest);
    assert!(report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim == "signature_verification"));
    assert!(!report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim == "fallback_nonce_freshness_or_uniqueness"));
}

#[test]
fn verify_mcp_records_fails_when_outcome_binds_different_decision() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let attestation_digest = attestation_digest();
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(&attestation_digest)).unwrap();
    fs::write(
        &outcome,
        outcome_json(&attestation_digest, "sha256:0000000000000000"),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("outcome_decision_digest_match"))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn verify_mcp_records_fails_on_substituted_backlink() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json("sha256:0000")).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_attestation_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn verify_mcp_records_accepts_request_envelope_fallback_pairing() {
    let dir = tempdir().unwrap();
    let request_envelope = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let envelope_digest = jcs_digest_json(request_envelope_json());
    let decision_body = decision_json(&envelope_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(&outcome, outcome_json(&envelope_digest, &decision_digest)).unwrap();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            request_envelope.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["binding"]["mode"], "request_envelope");
    assert_eq!(report["binding"]["digest"], envelope_digest);
    assert_eq!(report["binding"]["digest_source"], "request_envelope_jcs");
    let binding_nonce = binding_nonce();
    assert_eq!(
        report["binding"]["nonce"].as_str(),
        Some(binding_nonce.as_str())
    );
    assert_eq!(
        report["binding"]["nonce_source"],
        "record_backlink_consistency"
    );
    assert_eq!(report["attestation"], Value::Null);
    assert!(report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim == "fallback_nonce_freshness_or_uniqueness"));
}

#[test]
fn verify_mcp_records_fallback_fails_on_decision_envelope_digest_substitution() {
    let dir = tempdir().unwrap();
    let request_envelope = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let envelope_digest = jcs_digest_json(request_envelope_json());
    let decision_body = decision_json("sha256:0000");
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(&outcome, outcome_json(&envelope_digest, &decision_digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            request_envelope.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_request_envelope_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn verify_mcp_records_fallback_fails_on_outcome_envelope_digest_substitution() {
    let dir = tempdir().unwrap();
    let request_envelope = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let envelope_digest = jcs_digest_json(request_envelope_json());
    let decision_body = decision_json(&envelope_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(&outcome, outcome_json("sha256:0000", &decision_digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            request_envelope.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "outcome_request_envelope_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn verify_mcp_records_fallback_fails_on_outcome_nonce_substitution() {
    let dir = tempdir().unwrap();
    let request_envelope = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let envelope_digest = jcs_digest_json(request_envelope_json());
    let decision_body = decision_json(&envelope_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    let substituted_nonce = substituted_binding_nonce();
    fs::write(
        &outcome,
        outcome_json_with_backlink(&envelope_digest, &substituted_nonce, &decision_digest),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            request_envelope.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--outcome",
            outcome.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("decision_outcome_backlink_match"))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn verify_mcp_records_requires_exactly_one_binding_input() {
    let dir = tempdir().unwrap();
    let request_envelope = dir.path().join("request-envelope.json");
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let attestation_digest = attestation_digest();
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(&attestation_digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--decision",
            decision.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "<--attestation <ATTESTATION>|--request-envelope <REQUEST_ENVELOPE>>",
        ))
        .stderr(predicate::str::contains(
            "Usage: assay evidence verify-mcp-records",
        ));

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--request-envelope",
            request_envelope.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the argument '--attestation <ATTESTATION>' cannot be used with '--request-envelope <REQUEST_ENVELOPE>'",
        ));
}

#[test]
fn verify_mcp_records_accepts_decision_only_pairing() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let attestation_digest = attestation_digest();
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(&attestation_digest)).unwrap();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["outcome"], Value::Null);
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| { check["id"] == "outcome_absent" && check["ok"] == true }));
}

#[test]
fn verify_mcp_records_fails_on_unknown_decision_enum() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let attestation_digest = attestation_digest();
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(
        &decision,
        decision_json_with_value(&attestation_digest, "defer"),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--attestation",
            attestation.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("decision_enum"))
        .stdout(predicate::str::contains(
            "defer is not one of allow, block, escalate",
        ));
}
