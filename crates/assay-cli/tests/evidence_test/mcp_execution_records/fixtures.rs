use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const ATTESTATION_DIGEST: &str =
    "sha256:eb86c33e0905be8dad78dbd2d795711631a2f893ed48c2942679b94c24e9dfc1";

pub(super) fn attestation_json() -> &'static str {
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

pub(super) fn decision_json(digest: &str) -> String {
    decision_json_with_value(digest, "allow")
}

pub(super) fn decision_json_with_value(digest: &str, decision: &str) -> String {
    decision_json_with_backlink(digest, "nonce-1", decision)
}

pub(super) fn decision_json_with_backlink(digest: &str, nonce: &str, decision: &str) -> String {
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

pub(super) fn outcome_json(digest: &str, decision_digest: &str) -> String {
    outcome_json_with_backlink(digest, "nonce-1", decision_digest)
}

pub(super) fn outcome_json_with_backlink(
    digest: &str,
    nonce: &str,
    decision_digest: &str,
) -> String {
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

pub(super) fn request_envelope_json() -> &'static str {
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

pub(super) fn jcs_digest_json(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap();
    jcs_digest_value(&value)
}

pub(super) fn jcs_digest_value(value: &Value) -> String {
    let canonical = assay_core::mcp::jcs::to_vec(value).unwrap();
    format!("sha256:{}", hex::encode(Sha256::digest(&canonical)))
}
