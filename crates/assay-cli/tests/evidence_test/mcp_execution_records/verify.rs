use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

use super::fixtures::{
    attestation_digest, attestation_json, binding_nonce, decision_json, decision_json_with_value,
    hash_only_projection, jcs_digest_json, outcome_json, outcome_json_with_backlink,
    outcome_json_with_commitment, request_envelope_json, sha256_of_str, substituted_binding_nonce,
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

/// SEP-2828 verification step 5. An `ArgsProjection` is fully checkable by a record-only consumer:
/// `projectionDigest` is sha256 over the bytes of the `projection` string, so nothing external is
/// needed. What the projection *commits to* is a different question, and stays declared-not-claimed.
#[test]
fn verify_mcp_records_recomputes_result_commitment_projection_digest() {
    let report = run_with_commitment("executed", &{
        let projection = hash_only_projection();
        let digest = sha256_of_str(&projection);
        format!(
            r#"{{"projection": {}, "projectionDigest": "{}"}}"#,
            serde_json::to_string(&projection).unwrap(),
            digest
        )
    });

    assert_eq!(report["ok"], true);
    assert!(check_ok(
        &report,
        "result_commitment_projection_digest_match"
    ));
    let commitment = &report["outcome"]["result_commitment"];
    assert_eq!(commitment["kind"], "args_projection");
    assert_eq!(
        commitment["embedded_digest"],
        "sha256:8b7262647fbf76fb7ae30d664e65069eaffc35aa793718beaee239309c9055cf"
    );
    // The committed value is never compared against a runtime result, and says so.
    assert!(claims(&report).contains(&"result_commitment_payload_binding".to_string()));
}

#[test]
fn verify_mcp_records_fails_on_result_commitment_projection_digest_mismatch() {
    let projection = hash_only_projection();
    let report = run_with_commitment(
        "executed",
        &format!(
            r#"{{"projection": {}, "projectionDigest": "sha256:{}"}}"#,
            serde_json::to_string(&projection).unwrap(),
            "0".repeat(64)
        ),
    );

    assert_eq!(report["ok"], false);
    assert!(!check_ok(
        &report,
        "result_commitment_projection_digest_match"
    ));
}

/// An `ArgsRef` addresses content this verifier never fetches. That is not a silent pass: the
/// undereferenced reference is named in `claims_not_made`.
#[test]
fn verify_mcp_records_declares_unfetched_result_commitment_ref() {
    let report = run_with_commitment(
        "executed",
        r#"{"ref": "https://example.test/result", "digest": "sha256:abc", "canonicalization": "jcs"}"#,
    );

    assert_eq!(report["ok"], true);
    assert_eq!(report["outcome"]["result_commitment"]["kind"], "args_ref");
    let claims = claims(&report);
    assert!(claims.contains(&"result_commitment_ref_not_dereferenced".to_string()));
    assert!(claims.contains(&"result_commitment_payload_binding".to_string()));
}

/// A refusal has no result, so a commitment on a refused outcome is a producer defect.
#[test]
fn verify_mcp_records_fails_when_refused_outcome_carries_a_result_commitment() {
    let projection = hash_only_projection();
    let report = run_with_commitment(
        "refused",
        &format!(
            r#"{{"projection": {}, "projectionDigest": "{}"}}"#,
            serde_json::to_string(&projection).unwrap(),
            sha256_of_str(&projection)
        ),
    );

    assert_eq!(report["ok"], false);
    assert!(!check_ok(&report, "result_commitment_absent_for_refused"));
}

#[test]
fn verify_mcp_records_reports_absent_commitment_for_refused_outcome() {
    let attestation_digest = attestation_digest();
    let decision_body = decision_json(&attestation_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    let nonce = binding_nonce();
    let outcome_body = outcome_json_with_backlink(&attestation_digest, &nonce, &decision_digest)
        .replace("\"status\": \"executed\"", "\"status\": \"refused\"");
    let report = run_report(&decision_body, &outcome_body);

    assert_eq!(report["ok"], true);
    assert!(check_ok(&report, "result_commitment_absent_for_refused"));
    assert_eq!(report["outcome"]["result_commitment"], Value::Null);
}

fn run_with_commitment(status: &str, commitment: &str) -> Value {
    let attestation_digest = attestation_digest();
    let decision_body = decision_json(&attestation_digest);
    let decision_digest = jcs_digest_json(&decision_body);
    let outcome_body =
        outcome_json_with_commitment(&attestation_digest, &decision_digest, status, commitment);
    run_report(&decision_body, &outcome_body)
}

fn run_report(decision_body: &str, outcome_body: &str) -> Value {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(&outcome, outcome_body).unwrap();

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
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

fn check_ok(report: &Value, id: &str) -> bool {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .map(|c| c["ok"] == true)
        .unwrap_or(false)
}

fn claims(report: &Value) -> Vec<String> {
    report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}
