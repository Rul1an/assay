use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::tempdir;

const ATTESTATION_DIGEST: &str =
    "sha256:eb86c33e0905be8dad78dbd2d795711631a2f893ed48c2942679b94c24e9dfc1";

fn attestation_json() -> &'static str {
    r#"{
  "version": 1,
  "alg": "ES256",
  "issuerAsserted": {
    "iss": "issuer",
    "sub": "agent:test",
    "iat": "2026-06-01T00:00:00Z",
    "nonce": "nonce-1",
    "secretVersion": "test",
    "alg": "ES256"
  },
  "plannerDeclared": {
    "intent": "test"
  },
  "payloadDerived": {
    "toolCalls": [
      {
        "name": "tools/call",
        "serverFingerprint": "srv",
        "argsProjection": {
          "projection": "{\"digest\":\"sha256:abc\"}",
          "projectionDigest": "sha256:def"
        }
      }
    ]
  },
  "signature": "sig"
}"#
}

fn decision_json(digest: &str) -> String {
    decision_json_with_value(digest, "allow")
}

fn decision_json_with_value(digest: &str, decision: &str) -> String {
    decision_json_with_backlink(digest, "nonce-1", decision)
}

fn decision_json_with_backlink(digest: &str, nonce: &str, decision: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "backLink": {{
    "attestationDigest": "{digest}",
    "attestationNonce": "{nonce}"
  }},
  "decisionDerived": {{
    "decision": "{decision}",
    "policyId": "policy:test",
    "decidedAt": "2026-06-01T00:00:01Z"
  }},
  "issuerAsserted": {{
    "iss": "server",
    "sub": "agent:test",
    "iat": "2026-06-01T00:00:01Z",
    "nonce": "decision-1",
    "secretVersion": "test",
    "alg": "ES256"
  }},
  "signature": "decision-sig"
}}"#
    )
}

fn outcome_json(digest: &str, decision_digest: &str) -> String {
    outcome_json_with_backlink(digest, "nonce-1", decision_digest)
}

fn outcome_json_with_backlink(digest: &str, nonce: &str, decision_digest: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "backLink": {{
    "attestationDigest": "{digest}",
    "attestationNonce": "{nonce}"
  }},
  "outcomeDerived": {{
    "status": "executed",
    "completedAt": "2026-06-01T00:00:02Z",
    "decisionDigest": "{decision_digest}"
  }},
  "receiptAsserted": {{
    "iss": "server",
    "sub": "agent:test",
    "iat": "2026-06-01T00:00:02Z",
    "nonce": "outcome-1",
    "secretVersion": "test",
    "alg": "ES256"
  }},
  "signature": "outcome-sig"
}}"#
    )
}

fn request_envelope_json() -> &'static str {
    r#"{
  "name": "tools/call",
  "arguments": {
    "processInstanceKey": "2251799813685249"
  },
  "_meta": {
    "callId": "call-001",
    "requestSource": "mcp"
  }
}"#
}

fn jcs_digest_json(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap();
    let canonical = assay_core::mcp::jcs::to_vec(&value).unwrap();
    format!("sha256:{}", hex::encode(Sha256::digest(&canonical)))
}

#[test]
fn verify_mcp_records_reports_pairing_as_independent_consumer() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    let decision_body = decision_json(ATTESTATION_DIGEST);
    let decision_digest = jcs_digest_json(&decision_body);
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_body).unwrap();
    fs::write(&outcome, outcome_json(ATTESTATION_DIGEST, &decision_digest)).unwrap();

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
    assert_eq!(report["binding"]["digest"], ATTESTATION_DIGEST);
    assert_eq!(report["binding"]["nonce"], "nonce-1");
    assert_eq!(report["binding"]["nonce_source"], "issuerAsserted.nonce");
    assert_eq!(report["attestation"]["digest"], ATTESTATION_DIGEST);
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
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(ATTESTATION_DIGEST)).unwrap();
    fs::write(
        &outcome,
        outcome_json(ATTESTATION_DIGEST, "sha256:0000000000000000"),
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
    assert_eq!(report["binding"]["nonce"], "nonce-1");
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
    fs::write(
        &outcome,
        outcome_json_with_backlink(&envelope_digest, "nonce-2", &decision_digest),
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
    fs::write(&request_envelope, request_envelope_json()).unwrap();
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(ATTESTATION_DIGEST)).unwrap();

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
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(ATTESTATION_DIGEST)).unwrap();

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
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(
        &decision,
        decision_json_with_value(ATTESTATION_DIGEST, "defer"),
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
