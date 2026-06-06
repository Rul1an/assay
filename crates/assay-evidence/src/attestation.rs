//! In-toto / DSSE attestation over an evidence bundle manifest (ADR-039).
//!
//! Wraps a bundle [`Manifest`] as an in-toto v1 Statement and signs it as a DSSE
//! envelope, reusing the mandate DSSE primitives (PAE + Ed25519). The anchor
//! (a transparency log or timestamp) stays pluggable and external.
//!
//! Honest boundary: an attestation binds who-said-it and the bundle content. It
//! does NOT upgrade observed support, and provides no trust root or transparency
//! log on its own.

use crate::bundle::Manifest;
use crate::crypto::jcs;
use crate::mandate::signing::{build_pae, compute_key_id_from_verifying_key};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// in-toto Statement type URI (v1).
const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
/// DSSE payload type for in-toto statements.
const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";
/// Assay evidence-bundle predicate type (v0; not a frozen public spec).
pub const EVIDENCE_BUNDLE_PREDICATE_TYPE: &str = "https://assay.dev/attestation/evidence-bundle/v0";

/// in-toto subject: a named artifact plus its content digest(s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subject {
    pub name: String,
    pub digest: BTreeMap<String, String>,
}

/// in-toto v1 Statement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InTotoStatement {
    #[serde(rename = "_type")]
    pub type_: String,
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: serde_json::Value,
}

/// Build an in-toto v1 Statement whose subject is the bundle's integrity root.
pub fn statement_from_manifest(
    manifest: &Manifest,
    predicate: serde_json::Value,
) -> InTotoStatement {
    let digest_hex = manifest
        .run_root
        .strip_prefix("sha256:")
        .unwrap_or(&manifest.run_root)
        .to_string();
    let mut digest = BTreeMap::new();
    digest.insert("sha256".to_string(), digest_hex);
    InTotoStatement {
        type_: STATEMENT_TYPE.to_string(),
        subject: vec![Subject {
            name: manifest.bundle_id.clone(),
            digest,
        }],
        predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE.to_string(),
        predicate,
    }
}

/// A single DSSE signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseSignature {
    pub keyid: String,
    pub sig: String,
}

/// A DSSE envelope carrying an in-toto attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseEnvelope {
    pub payload: String,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub signatures: Vec<DsseSignature>,
}

/// Sign an in-toto Statement as a DSSE envelope (Ed25519 over the DSSE PAE of the
/// JCS-canonicalized statement).
pub fn sign_statement(statement: &InTotoStatement, key: &SigningKey) -> Result<DsseEnvelope> {
    let canonical = jcs::to_vec(statement).context("canonicalize in-toto statement")?;
    let pae = build_pae(IN_TOTO_PAYLOAD_TYPE, &canonical);
    let signature: Ed25519Signature = key.sign(&pae);
    let keyid = compute_key_id_from_verifying_key(&key.verifying_key())?;
    Ok(DsseEnvelope {
        payload: BASE64.encode(&canonical),
        payload_type: IN_TOTO_PAYLOAD_TYPE.to_string(),
        signatures: vec![DsseSignature {
            keyid,
            sig: BASE64.encode(signature.to_bytes()),
        }],
    })
}

/// Verify a DSSE envelope against a trusted key and return the contained Statement.
pub fn verify_envelope(
    envelope: &DsseEnvelope,
    trusted_key: &VerifyingKey,
) -> Result<InTotoStatement> {
    // Reject any DSSE payload type other than in-toto BEFORE verifying, so a key
    // that signed the same bytes under a different payload type cannot be accepted
    // as an in-toto attestation (payload-type confusion). The PAE binds the type,
    // so we must verify under the type we require, not the one the envelope claims.
    if envelope.payload_type != IN_TOTO_PAYLOAD_TYPE {
        anyhow::bail!(
            "unexpected DSSE payloadType: expected {}, got {}",
            IN_TOTO_PAYLOAD_TYPE,
            envelope.payload_type
        );
    }
    let canonical = BASE64
        .decode(&envelope.payload)
        .context("decode dsse payload")?;
    let pae = build_pae(IN_TOTO_PAYLOAD_TYPE, &canonical);
    let dsse_sig = envelope
        .signatures
        .first()
        .context("dsse envelope has no signatures")?;
    let sig_bytes = BASE64.decode(&dsse_sig.sig).context("decode signature")?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature is not 64 bytes"))?;
    let signature = Ed25519Signature::from_bytes(&sig_array);
    trusted_key
        .verify(&pae, &signature)
        .context("dsse signature verification failed")?;
    let statement: InTotoStatement =
        serde_json::from_slice(&canonical).context("parse in-toto statement")?;
    // Defense in depth: the verified payload must be a v1 in-toto Statement.
    if statement.type_ != STATEMENT_TYPE {
        anyhow::bail!(
            "unexpected in-toto statement _type: expected {}, got {}",
            STATEMENT_TYPE,
            statement.type_
        );
    }
    Ok(statement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_statement() -> InTotoStatement {
        let mut digest = BTreeMap::new();
        digest.insert("sha256".to_string(), "abc123".to_string());
        InTotoStatement {
            type_: STATEMENT_TYPE.to_string(),
            subject: vec![Subject {
                name: "bundle-1".to_string(),
                digest,
            }],
            predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE.to_string(),
            predicate: serde_json::json!({ "event_count": 3, "outcome": "supported" }),
        }
    }

    #[test]
    fn sign_then_verify_roundtrips_and_detects_tamper() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let statement = sample_statement();

        let envelope = sign_statement(&statement, &key).expect("sign");
        assert_eq!(envelope.payload_type, IN_TOTO_PAYLOAD_TYPE);

        let recovered = verify_envelope(&envelope, &key.verifying_key()).expect("verify");
        assert_eq!(recovered, statement);

        // Tampering with the payload must fail verification.
        let mut tampered = envelope.clone();
        let mut bytes = BASE64.decode(&tampered.payload).unwrap();
        bytes[0] ^= 0xFF;
        tampered.payload = BASE64.encode(&bytes);
        assert!(verify_envelope(&tampered, &key.verifying_key()).is_err());

        // A different key must fail verification.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        assert!(verify_envelope(&envelope, &other.verifying_key()).is_err());
    }

    #[test]
    fn verify_rejects_non_in_toto_payload_type() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let statement = sample_statement();
        let mut envelope = sign_statement(&statement, &key).expect("sign");

        // Re-label the envelope as a different DSSE payload type. Even with a
        // genuine signature over the same bytes, an in-toto verifier must reject it.
        envelope.payload_type = "application/json".to_string();
        let err = verify_envelope(&envelope, &key.verifying_key())
            .expect_err("must reject non-in-toto payload type");
        assert!(err.to_string().contains("payloadType"));
    }
}
