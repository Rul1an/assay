//! Mandate Signing and Verification (SPEC-Mandate-v1 §4)
//!
//! DSSE-compatible signing for mandate evidence.
//!
//! # Signing Process
//!
//! ```text
//! 1. Build hashable_content = data object WITHOUT {mandate_id, signature}
//! 2. Compute canonical_for_id = JCS(hashable_content)
//! 3. Compute mandate_id = "sha256:" + hex(SHA256(canonical_for_id))
//! 4. Build signable_content = hashable_content + {mandate_id: mandate_id}
//! 5. Compute canonical_for_sig = JCS(signable_content)
//! 6. Compute PAE = DSSEv1_PAE(payload_type, canonical_for_sig)
//! 7. Sign: signature_bytes = ed25519_sign(private_key, PAE)
//! 8. Build signature object with payload_digest = mandate_id
//! ```

use crate::crypto::jcs;
use crate::mandate::id::compute_mandate_id;
use crate::mandate::types::{Mandate, MandateContent, Signature, MANDATE_PAYLOAD_TYPE};
use anyhow::{Context as AnyhowContext, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Verification errors with exit codes per SPEC-Mandate-v1 §5.5.
#[derive(Debug, Clone, thiserror::Error)]
pub enum VerifyError {
    #[error("mandate is not signed")]
    Unsigned,

    #[error("malformed signature: {reason}")]
    Malformed { reason: String },

    #[error("signature version mismatch: expected 1, got {version}")]
    VersionMismatch { version: u32 },

    #[error("algorithm mismatch: expected ed25519, got {algorithm}")]
    AlgorithmMismatch { algorithm: String },

    #[error("payload type mismatch: expected {expected}, got {got}")]
    PayloadTypeMismatch { expected: String, got: String },

    #[error("mandate_id does not match content_id")]
    IdContentMismatch,

    #[error("signed_payload_digest mismatch: computed {computed}, claimed {claimed}")]
    SignedPayloadDigestMismatch { computed: String, claimed: String },

    #[error("computed mandate_id does not match claimed: computed {computed}, claimed {claimed}")]
    IdMismatch { computed: String, claimed: String },

    #[error("signature verification failed")]
    SignatureInvalid,

    #[error("key not trusted: {key_id}")]
    KeyNotTrusted { key_id: String },

    #[error("key_id mismatch: claimed {claimed}, actual {actual}")]
    KeyIdMismatch { claimed: String, actual: String },
}

impl VerifyError {
    /// Exit code per SPEC-Mandate-v1 §5.5.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Unsigned => 2,
            Self::KeyNotTrusted { .. } => 3,
            Self::SignatureInvalid
            | Self::PayloadTypeMismatch { .. }
            | Self::IdContentMismatch
            | Self::IdMismatch { .. }
            | Self::KeyIdMismatch { .. }
            | Self::SignedPayloadDigestMismatch { .. } => 4,
            Self::Malformed { .. }
            | Self::VersionMismatch { .. }
            | Self::AlgorithmMismatch { .. } => 1,
        }
    }
}

/// Result of successful verification.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub mandate_id: String,
    pub key_id: String,
    pub signed_at: chrono::DateTime<Utc>,
}

/// Compute key_id from SPKI-encoded public key bytes.
///
/// Returns `sha256:<lowercase-hex>`.
pub fn compute_key_id(spki_bytes: &[u8]) -> String {
    let hash = Sha256::digest(spki_bytes);
    format!("sha256:{:x}", hash)
}

/// Compute key_id from a VerifyingKey.
pub fn compute_key_id_from_verifying_key(key: &VerifyingKey) -> Result<String> {
    let spki_bytes = key_to_spki_der(key)?;
    Ok(compute_key_id(&spki_bytes))
}

/// Convert VerifyingKey to SPKI DER bytes.
fn key_to_spki_der(key: &VerifyingKey) -> Result<Vec<u8>> {
    use ed25519_dalek::pkcs8::EncodePublicKey;
    let doc = key
        .to_public_key_der()
        .map_err(|e| anyhow::anyhow!("failed to encode public key as SPKI DER: {}", e))?;
    Ok(doc.as_bytes().to_vec())
}

/// Build DSSE Pre-Authentication Encoding (PAE).
///
/// ```text
/// PAE(type, payload) = "DSSEv1" SP LEN(type) SP type SP LEN(payload) SP payload
/// ```
fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let type_len = payload_type.len().to_string();
    let payload_len = payload.len().to_string();

    let mut pae = Vec::new();
    pae.extend_from_slice(b"DSSEv1 ");
    pae.extend_from_slice(type_len.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_type.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_len.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload);
    pae
}

/// Sign a mandate.
///
/// # Arguments
///
/// * `content` - Mandate content (without mandate_id)
/// * `signing_key` - Ed25519 private key
///
/// # Returns
///
/// Complete signed Mandate with mandate_id and signature.
///
/// # Algorithm
///
/// 1. Compute mandate_id from content (JCS + SHA256)
/// 2. Build signable content (content + mandate_id)
/// 3. Canonicalize signable content
/// 4. Build PAE and sign
/// 5. Return complete Mandate
pub fn sign_mandate(content: &MandateContent, signing_key: &SigningKey) -> Result<Mandate> {
    // 1. Compute mandate_id from content (excludes mandate_id and signature)
    let mandate_id = compute_mandate_id(content)?;

    // 2. Build signable content (content + mandate_id, no signature yet)
    let signable = SignableMandate {
        mandate_id: mandate_id.clone(),
        mandate_kind: content.mandate_kind,
        principal: content.principal.clone(),
        scope: content.scope.clone(),
        validity: content.validity.clone(),
        constraints: content.constraints.clone(),
        context: content.context.clone(),
    };

    // 3. Canonicalize for signing
    let canonical = jcs::to_vec(&signable).context("failed to canonicalize mandate for signing")?;

    // 4. Compute signed_payload_digest (DSSE standard: digest of signed payload)
    let signed_payload_digest = format!("sha256:{}", hex::encode(Sha256::digest(&canonical)));

    // 5. Build PAE
    let pae = build_pae(MANDATE_PAYLOAD_TYPE, &canonical);

    // 6. Sign
    let signature: ed25519_dalek::Signature = signing_key.sign(&pae);

    // 7. Build signature object
    let verifying_key = signing_key.verifying_key();
    let key_id = compute_key_id_from_verifying_key(&verifying_key)?;

    let sig = Signature {
        version: 1,
        algorithm: "ed25519".to_string(),
        payload_type: MANDATE_PAYLOAD_TYPE.to_string(),
        content_id: mandate_id.clone(),
        signed_payload_digest,
        key_id,
        signature: BASE64.encode(signature.to_bytes()),
        signed_at: Utc::now(),
    };

    // 8. Build complete mandate
    Ok(Mandate {
        mandate_id,
        mandate_kind: content.mandate_kind,
        principal: content.principal.clone(),
        scope: content.scope.clone(),
        validity: content.validity.clone(),
        constraints: content.constraints.clone(),
        context: content.context.clone(),
        signature: Some(sig),
    })
}

/// Signable mandate (content + mandate_id, no signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignableMandate {
    mandate_id: String,
    mandate_kind: crate::mandate::types::MandateKind,
    principal: crate::mandate::types::Principal,
    scope: crate::mandate::types::Scope,
    validity: crate::mandate::types::Validity,
    constraints: crate::mandate::types::Constraints,
    context: crate::mandate::types::Context,
}

/// Verify a signed mandate.
///
/// # Arguments
///
/// * `mandate` - Signed mandate to verify
/// * `trusted_key` - Public key to verify against
///
/// # Returns
///
/// `VerifyResult` on success, `VerifyError` on failure.
///
/// # Algorithm (SPEC-Mandate-v1 §5.1 v1.0.2)
///
/// 1. Extract and validate signature
/// 2. Verify mandate_id == content_id
/// 3. Recompute mandate_id from content (proves content-addressed)
/// 4. Verify signed_payload_digest
/// 5. Build signable content and PAE
/// 6. Verify ed25519 signature
/// 7. Verify key_id matches
pub fn verify_mandate(
    mandate: &Mandate,
    trusted_key: &VerifyingKey,
) -> Result<VerifyResult, VerifyError> {
    // 1. Extract signature
    let sig = mandate.signature.as_ref().ok_or(VerifyError::Unsigned)?;

    // 2. Validate signature fields
    if sig.version != 1 {
        return Err(VerifyError::VersionMismatch {
            version: sig.version,
        });
    }
    if sig.algorithm != "ed25519" {
        return Err(VerifyError::AlgorithmMismatch {
            algorithm: sig.algorithm.clone(),
        });
    }
    if sig.payload_type != MANDATE_PAYLOAD_TYPE {
        return Err(VerifyError::PayloadTypeMismatch {
            expected: MANDATE_PAYLOAD_TYPE.to_string(),
            got: sig.payload_type.clone(),
        });
    }

    // 3. Verify mandate_id == content_id (v1.0.2)
    if mandate.mandate_id != sig.content_id {
        return Err(VerifyError::IdContentMismatch);
    }

    // 4. Recompute mandate_id from content (proves content-addressed)
    let content = MandateContent {
        mandate_kind: mandate.mandate_kind,
        principal: mandate.principal.clone(),
        scope: mandate.scope.clone(),
        validity: mandate.validity.clone(),
        constraints: mandate.constraints.clone(),
        context: mandate.context.clone(),
    };
    let computed_id = compute_mandate_id(&content).map_err(|e| VerifyError::Malformed {
        reason: e.to_string(),
    })?;

    if computed_id != mandate.mandate_id {
        return Err(VerifyError::IdMismatch {
            computed: computed_id,
            claimed: mandate.mandate_id.clone(),
        });
    }

    // 5. Build signable content (content + mandate_id)
    let signable = SignableMandate {
        mandate_id: mandate.mandate_id.clone(),
        mandate_kind: mandate.mandate_kind,
        principal: mandate.principal.clone(),
        scope: mandate.scope.clone(),
        validity: mandate.validity.clone(),
        constraints: mandate.constraints.clone(),
        context: mandate.context.clone(),
    };

    let canonical = jcs::to_vec(&signable).map_err(|e| VerifyError::Malformed {
        reason: e.to_string(),
    })?;

    // 6. Verify signed_payload_digest (v1.0.2 DSSE alignment)
    let computed_signed_digest = format!("sha256:{}", hex::encode(Sha256::digest(&canonical)));
    if computed_signed_digest != sig.signed_payload_digest {
        return Err(VerifyError::SignedPayloadDigestMismatch {
            computed: computed_signed_digest,
            claimed: sig.signed_payload_digest.clone(),
        });
    }

    // 7. Build PAE and verify signature
    let pae = build_pae(&sig.payload_type, &canonical);

    let signature_bytes = BASE64
        .decode(&sig.signature)
        .map_err(|e| VerifyError::Malformed {
            reason: format!("invalid base64 signature: {}", e),
        })?;

    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes).map_err(|e| {
        VerifyError::Malformed {
            reason: format!("invalid signature bytes: {}", e),
        }
    })?;

    trusted_key
        .verify(&pae, &signature)
        .map_err(|_| VerifyError::SignatureInvalid)?;

    // 7. Verify key_id matches
    let actual_key_id =
        compute_key_id_from_verifying_key(trusted_key).map_err(|e| VerifyError::Malformed {
            reason: e.to_string(),
        })?;

    if sig.key_id != actual_key_id {
        return Err(VerifyError::KeyIdMismatch {
            claimed: sig.key_id.clone(),
            actual: actual_key_id,
        });
    }

    Ok(VerifyResult {
        mandate_id: mandate.mandate_id.clone(),
        key_id: sig.key_id.clone(),
        signed_at: sig.signed_at,
    })
}

/// Check if a mandate is signed.
pub fn is_signed(mandate: &Mandate) -> bool {
    mandate.signature.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mandate::types::{
        AuthMethod, Constraints, Context as MandateContext, MandateContent, MandateKind, Principal,
        Scope, Validity,
    };
    use chrono::TimeZone;

    fn generate_keypair() -> SigningKey {
        SigningKey::generate(&mut rand::thread_rng())
    }

    fn create_test_content() -> MandateContent {
        MandateContent {
            mandate_kind: MandateKind::Intent,
            principal: Principal::new("user-123", AuthMethod::Oidc),
            scope: Scope::new(vec!["search_*".to_string()]),
            validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
            constraints: Constraints::default(),
            context: MandateContext::new("myorg/app", "auth.myorg.com"),
        }
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let key = generate_keypair();
        let content = create_test_content();

        let signed = sign_mandate(&content, &key).unwrap();

        // Verify signature is present
        assert!(is_signed(&signed));
        assert!(signed.mandate_id.starts_with("sha256:"));

        // Verify roundtrip
        let result = verify_mandate(&signed, &key.verifying_key()).unwrap();
        assert_eq!(result.mandate_id, signed.mandate_id);
        assert!(result.key_id.starts_with("sha256:"));
    }

    #[test]
    fn test_tamper_detection_payload() {
        let key = generate_keypair();
        let content = create_test_content();

        let mut signed = sign_mandate(&content, &key).unwrap();

        // Tamper with content
        signed.principal.subject = "attacker".to_string();

        let result = verify_mandate(&signed, &key.verifying_key());
        assert!(matches!(result, Err(VerifyError::IdMismatch { .. })));
    }

    #[test]
    fn test_tamper_detection_mandate_id() {
        let key = generate_keypair();
        let content = create_test_content();

        let mut signed = sign_mandate(&content, &key).unwrap();

        // Tamper with mandate_id
        signed.mandate_id =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let result = verify_mandate(&signed, &key.verifying_key());
        // Could be IdDigestMismatch or IdMismatch depending on order of checks
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_keypair();
        let key2 = generate_keypair();
        let content = create_test_content();

        let signed = sign_mandate(&content, &key1).unwrap();

        let result = verify_mandate(&signed, &key2.verifying_key());
        assert!(matches!(
            result,
            Err(VerifyError::SignatureInvalid) | Err(VerifyError::KeyIdMismatch { .. })
        ));
    }

    #[test]
    fn test_unsigned_mandate() {
        let key = generate_keypair();
        let content = create_test_content();

        // Create unsigned mandate
        let mandate_id = compute_mandate_id(&content).unwrap();
        let unsigned = content.into_mandate(mandate_id);

        let result = verify_mandate(&unsigned, &key.verifying_key());
        assert!(matches!(result, Err(VerifyError::Unsigned)));
    }

    #[test]
    fn test_mandate_id_is_content_addressed() {
        let key = generate_keypair();
        let content = create_test_content();

        // Sign twice - should get same mandate_id
        let signed1 = sign_mandate(&content, &key).unwrap();
        let signed2 = sign_mandate(&content, &key).unwrap();

        assert_eq!(signed1.mandate_id, signed2.mandate_id);
    }

    #[test]
    fn test_payload_type_length() {
        // Normative test: payload type must be specific length
        assert!(MANDATE_PAYLOAD_TYPE.is_ascii());
        assert_eq!(
            MANDATE_PAYLOAD_TYPE,
            "application/vnd.assay.mandate+json;v=1"
        );
    }

    #[test]
    fn test_exit_codes() {
        assert_eq!(VerifyError::Unsigned.exit_code(), 2);
        assert_eq!(
            VerifyError::KeyNotTrusted { key_id: "x".into() }.exit_code(),
            3
        );
        assert_eq!(VerifyError::SignatureInvalid.exit_code(), 4);
        assert_eq!(VerifyError::Malformed { reason: "x".into() }.exit_code(), 1);
    }

    #[test]
    fn test_key_id_lowercase_hex() {
        let key = generate_keypair();
        let key_id = compute_key_id_from_verifying_key(&key.verifying_key()).unwrap();

        assert!(key_id.starts_with("sha256:"));
        let hex_part = &key_id[7..];
        assert!(
            hex_part
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "key_id hex must be lowercase: {}",
            key_id
        );
    }
}
