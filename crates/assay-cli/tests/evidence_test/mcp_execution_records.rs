use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
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
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "backLink": {{
    "attestationDigest": "{digest}",
    "attestationNonce": "nonce-1"
  }},
  "decisionDerived": {{
    "decision": "allow",
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

fn outcome_json(digest: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "backLink": {{
    "attestationDigest": "{digest}",
    "attestationNonce": "nonce-1"
  }},
  "outcomeDerived": {{
    "status": "executed",
    "completedAt": "2026-06-01T00:00:02Z"
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

#[test]
fn verify_mcp_records_reports_pairing_as_independent_consumer() {
    let dir = tempdir().unwrap();
    let attestation = dir.path().join("attestation.json");
    let decision = dir.path().join("decision.json");
    let outcome = dir.path().join("outcome.json");
    fs::write(&attestation, attestation_json()).unwrap();
    fs::write(&decision, decision_json(ATTESTATION_DIGEST)).unwrap();
    fs::write(&outcome, outcome_json(ATTESTATION_DIGEST)).unwrap();

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
    assert_eq!(report["attestation"]["digest"], ATTESTATION_DIGEST);
    assert_eq!(report["decision"]["decision"], "allow");
    assert_eq!(report["outcome"]["status"], "executed");
    assert!(report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim == "signature_verification"));
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
