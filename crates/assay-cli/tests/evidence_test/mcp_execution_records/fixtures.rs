use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn attestation_digest() -> String {
    jcs_digest_json(&attestation_json())
}

pub(super) fn binding_nonce() -> String {
    fixture_value("binding")
}

pub(super) fn substituted_binding_nonce() -> String {
    fixture_value("substituted-binding")
}

fn fixture_value(label: &str) -> String {
    let seed = format!("assay-mcp-execution-records/{label}");
    let digest = hex::encode(Sha256::digest(seed.as_bytes()));
    format!("fixture-{}", &digest[..16])
}

pub(super) fn attestation_json() -> String {
    let nonce = binding_nonce();
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "issuerAsserted": {{
    "iss": "issuer",
    "sub": "agent:test",
    "iat": "2026-06-01T00:00:00Z",
    "nonce": "{nonce}",
    "secretVersion": "test",
    "alg": "ES256"
  }},
  "plannerDeclared": {{
    "intent": "test"
  }},
  "payloadDerived": {{
    "toolCalls": [
      {{
        "name": "tools/call",
        "serverFingerprint": "srv",
        "argsProjection": {{
          "projection": "{{\"digest\":\"sha256:abc\"}}",
          "projectionDigest": "sha256:def"
        }}
      }}
    ]
  }},
  "signature": "sig"
}}"#
    )
}

pub(super) fn decision_json(digest: &str) -> String {
    decision_json_with_value(digest, "allow")
}

pub(super) fn decision_json_with_value(digest: &str, decision: &str) -> String {
    let nonce = binding_nonce();
    decision_json_with_backlink(digest, &nonce, decision)
}

pub(super) fn decision_json_with_backlink(digest: &str, nonce: &str, decision: &str) -> String {
    let issuer_nonce = fixture_value("decision-issuer");
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
    "nonce": "{issuer_nonce}",
    "secretVersion": "test",
    "alg": "ES256"
  }},
  "signature": "decision-sig"
}}"#
    )
}

pub(super) fn outcome_json(digest: &str, decision_digest: &str) -> String {
    let nonce = binding_nonce();
    outcome_json_with_backlink(digest, &nonce, decision_digest)
}

pub(super) fn outcome_json_with_backlink(
    digest: &str,
    nonce: &str,
    decision_digest: &str,
) -> String {
    let receipt_nonce = fixture_value("outcome-receipt");
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
    "nonce": "{receipt_nonce}",
    "secretVersion": "test",
    "alg": "ES256"
  }},
  "signature": "outcome-sig"
}}"#
    )
}

/// sha256 over raw UTF-8 bytes. SEP-2828 defines `projectionDigest` over the bytes of the
/// `projection` string itself, so this is deliberately not `jcs_digest_json`, which canonicalizes
/// a value first.
pub(super) fn sha256_of_str(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}

/// The RECOMMENDED hash-only-identity commitment: a projection whose content is the JCS encoding
/// of `{"digest": ...}`, committing to a result the record never carries.
pub(super) fn hash_only_projection() -> String {
    r#"{"digest":"sha256:8b7262647fbf76fb7ae30d664e65069eaffc35aa793718beaee239309c9055cf"}"#
        .to_string()
}

pub(super) fn outcome_json_with_commitment(
    digest: &str,
    decision_digest: &str,
    status: &str,
    commitment: &str,
) -> String {
    let nonce = binding_nonce();
    let receipt_nonce = fixture_value("outcome-receipt");
    format!(
        r#"{{
  "version": 1,
  "alg": "ES256",
  "backLink": {{
    "attestationDigest": "{digest}",
    "attestationNonce": "{nonce}"
  }},
  "outcomeDerived": {{
    "status": "{status}",
    "completedAt": "2026-06-01T00:00:02Z",
    "decisionDigest": "{decision_digest}",
    "resultCommitment": {commitment}
  }},
  "receiptAsserted": {{
    "iss": "server",
    "sub": "agent:test",
    "iat": "2026-06-01T00:00:02Z",
    "nonce": "{receipt_nonce}",
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
