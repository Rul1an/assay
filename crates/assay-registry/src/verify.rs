//! Pack verification (digest and signature).
//!
//! Implements verification per SPEC-Pack-Registry-v1:
//! - Digest verification (SHA-256 of JCS-canonical content)
//! - DSSE signature verification (Ed25519 over PAE)

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{RegistryError, RegistryResult};
use crate::trust::TrustStore;
use crate::types::{DsseEnvelope, FetchResult};

/// Payload type for pack definitions (DSSE-style binding).
pub const PAYLOAD_TYPE_PACK_V1: &str = "application/vnd.assay.pack+yaml;v=1";

/// Result of successful verification.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Whether the pack is signed.
    pub signed: bool,

    /// Key ID used for signing (if signed).
    pub key_id: Option<String>,

    /// Content digest.
    pub digest: String,
}

/// Verification options.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    /// Allow unsigned packs (for dev/testing).
    pub allow_unsigned: bool,

    /// Skip signature verification (verify digest only).
    pub skip_signature: bool,
}

impl VerifyOptions {
    /// Allow unsigned packs.
    pub fn allow_unsigned(mut self) -> Self {
        self.allow_unsigned = true;
        self
    }

    /// Skip signature verification.
    pub fn skip_signature(mut self) -> Self {
        self.skip_signature = true;
        self
    }
}

/// Verify a fetched pack.
///
/// # Verification Steps
///
/// 1. Compute SHA-256 digest of content
/// 2. Compare against `X-Pack-Digest` header (if present)
/// 3. If signed, verify DSSE signature over PAE
/// 4. Check key against trust store
///
/// # Arguments
///
/// * `result` - Fetch result from registry
/// * `trust_store` - Key trust store
/// * `options` - Verification options
pub fn verify_pack(
    result: &FetchResult,
    trust_store: &TrustStore,
    options: &VerifyOptions,
) -> RegistryResult<VerifyResult> {
    // 1. Verify digest
    if let Some(claimed_digest) = &result.headers.digest {
        if claimed_digest != &result.computed_digest {
            return Err(RegistryError::DigestMismatch {
                name: "pack".to_string(),
                version: "unknown".to_string(),
                expected: claimed_digest.clone(),
                actual: result.computed_digest.clone(),
            });
        }
    }

    // 2. Check for signature
    let signature = &result.headers.signature;
    if signature.is_none() {
        if options.allow_unsigned {
            return Ok(VerifyResult {
                signed: false,
                key_id: None,
                digest: result.computed_digest.clone(),
            });
        } else {
            return Err(RegistryError::Unsigned {
                name: "pack".to_string(),
                version: "unknown".to_string(),
            });
        }
    }

    // 3. Skip signature if requested
    if options.skip_signature {
        return Ok(VerifyResult {
            signed: true,
            key_id: result.headers.key_id.clone(),
            digest: result.computed_digest.clone(),
        });
    }

    // 4. Parse and verify DSSE signature
    let sig_b64 = signature.as_ref().unwrap();
    let envelope = parse_dsse_envelope(sig_b64)?;
    verify_dsse_signature(&result.content, &envelope, trust_store)?;

    Ok(VerifyResult {
        signed: true,
        key_id: envelope.signatures.first().map(|s| s.key_id.clone()),
        digest: result.computed_digest.clone(),
    })
}

/// Verify content digest matches expected.
pub fn verify_digest(content: &str, expected: &str) -> RegistryResult<()> {
    let computed = compute_digest(content);
    if computed != expected {
        return Err(RegistryError::DigestMismatch {
            name: "pack".to_string(),
            version: "unknown".to_string(),
            expected: expected.to_string(),
            actual: computed,
        });
    }
    Ok(())
}

/// Compute SHA-256 digest of content.
pub fn compute_digest(content: &str) -> String {
    let hash = Sha256::digest(content.as_bytes());
    format!("sha256:{:x}", hash)
}

/// Parse DSSE envelope from Base64.
fn parse_dsse_envelope(b64: &str) -> RegistryResult<DsseEnvelope> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| RegistryError::SignatureInvalid {
            reason: format!("invalid base64 envelope: {}", e),
        })?;

    serde_json::from_slice(&bytes).map_err(|e| RegistryError::SignatureInvalid {
        reason: format!("invalid DSSE envelope: {}", e),
    })
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

/// Verify DSSE signature over content.
fn verify_dsse_signature(
    content: &str,
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    // 1. Check payload type
    if envelope.payload_type != PAYLOAD_TYPE_PACK_V1 {
        return Err(RegistryError::SignatureInvalid {
            reason: format!(
                "payload type mismatch: expected {}, got {}",
                PAYLOAD_TYPE_PACK_V1, envelope.payload_type
            ),
        });
    }

    // 2. Decode and verify payload matches content
    let payload_bytes =
        BASE64
            .decode(&envelope.payload)
            .map_err(|e| RegistryError::SignatureInvalid {
                reason: format!("invalid base64 payload: {}", e),
            })?;
    let payload_str =
        String::from_utf8(payload_bytes.clone()).map_err(|e| RegistryError::SignatureInvalid {
            reason: format!("payload not valid UTF-8: {}", e),
        })?;

    // Content should match payload
    if payload_str != content {
        return Err(RegistryError::DigestMismatch {
            name: "pack".to_string(),
            version: "unknown".to_string(),
            expected: "envelope payload".to_string(),
            actual: "content".to_string(),
        });
    }

    // 3. Verify at least one signature
    if envelope.signatures.is_empty() {
        return Err(RegistryError::SignatureInvalid {
            reason: "no signatures in envelope".to_string(),
        });
    }

    // 4. Build PAE
    let pae = build_pae(&envelope.payload_type, &payload_bytes);

    // 5. Verify each signature until one succeeds
    let mut last_error = None;
    for sig in &envelope.signatures {
        match verify_single_signature(&pae, &sig.key_id, &sig.signature, trust_store) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e),
        }
    }

    Err(
        last_error.unwrap_or_else(|| RegistryError::SignatureInvalid {
            reason: "no valid signatures".to_string(),
        }),
    )
}

/// Verify a single signature.
fn verify_single_signature(
    pae: &[u8],
    key_id: &str,
    signature_b64: &str,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    // 1. Get key from trust store
    let key = trust_store.get_key(key_id)?;

    // 2. Decode signature
    let signature_bytes =
        BASE64
            .decode(signature_b64)
            .map_err(|e| RegistryError::SignatureInvalid {
                reason: format!("invalid base64 signature: {}", e),
            })?;

    let signature =
        Signature::from_slice(&signature_bytes).map_err(|e| RegistryError::SignatureInvalid {
            reason: format!("invalid signature bytes: {}", e),
        })?;

    // 3. Verify
    key.verify(pae, &signature)
        .map_err(|_| RegistryError::SignatureInvalid {
            reason: "ed25519 verification failed".to_string(),
        })
}

/// Compute key ID from public key bytes (SPKI DER).
pub fn compute_key_id(spki_bytes: &[u8]) -> String {
    let hash = Sha256::digest(spki_bytes);
    format!("sha256:{:x}", hash)
}

/// Compute key ID from a VerifyingKey.
pub fn compute_key_id_from_key(key: &VerifyingKey) -> RegistryResult<String> {
    use pkcs8::EncodePublicKey;
    let doc = key.to_public_key_der().map_err(|e| RegistryError::Config {
        message: format!("failed to encode public key: {}", e),
    })?;
    Ok(compute_key_id(doc.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn generate_keypair() -> SigningKey {
        SigningKey::generate(&mut rand::thread_rng())
    }

    #[test]
    fn test_compute_digest() {
        let content = "name: test\nversion: 1.0.0";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);
    }

    #[test]
    fn test_verify_digest_success() {
        let content = "test content";
        let expected = compute_digest(content);
        assert!(verify_digest(content, &expected).is_ok());
    }

    #[test]
    fn test_verify_digest_mismatch() {
        let content = "test content";
        let wrong = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_digest(content, wrong);
        assert!(matches!(result, Err(RegistryError::DigestMismatch { .. })));
    }

    #[test]
    fn test_build_pae() {
        let pae = build_pae("application/json", b"test");
        let expected = b"DSSEv1 16 application/json 4 test";
        assert_eq!(pae, expected);
    }

    #[test]
    fn test_payload_type_length() {
        // Verify payload type is correct length for PAE encoding
        // "application/vnd.assay.pack+yaml;v=1" is 35 bytes
        assert_eq!(
            PAYLOAD_TYPE_PACK_V1.len(),
            35,
            "PAYLOAD_TYPE_PACK_V1 must be 35 bytes"
        );
        assert!(PAYLOAD_TYPE_PACK_V1.is_ascii());

        // Verify PAE encoding uses correct length
        let pae = build_pae(PAYLOAD_TYPE_PACK_V1, b"{}");
        let pae_str = String::from_utf8_lossy(&pae);
        assert!(
            pae_str.starts_with("DSSEv1 35 application/vnd.assay.pack+yaml;v=1 2 {}"),
            "PAE must start with 'DSSEv1 35 ...' for pack signing"
        );
    }

    #[test]
    fn test_key_id_computation() {
        let key = generate_keypair();
        let key_id = compute_key_id_from_key(&key.verifying_key()).unwrap();

        assert!(key_id.starts_with("sha256:"));
        assert_eq!(key_id.len(), 7 + 64); // "sha256:" + 64 hex chars

        // Must be lowercase hex
        let hex_part = &key_id[7..];
        assert!(
            hex_part
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "key_id hex must be lowercase"
        );
    }

    #[test]
    fn test_parse_dsse_envelope_invalid_base64() {
        let result = parse_dsse_envelope("not valid base64!!!");
        assert!(matches!(
            result,
            Err(RegistryError::SignatureInvalid { .. })
        ));
    }

    #[test]
    fn test_parse_dsse_envelope_invalid_json() {
        let b64 = BASE64.encode(b"not json");
        let result = parse_dsse_envelope(&b64);
        assert!(matches!(
            result,
            Err(RegistryError::SignatureInvalid { .. })
        ));
    }

    #[test]
    fn test_parse_dsse_envelope_valid() {
        let envelope = DsseEnvelope {
            payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
            payload: BASE64.encode(b"test payload"),
            signatures: vec![],
        };
        let json = serde_json::to_vec(&envelope).unwrap();
        let b64 = BASE64.encode(&json);

        let parsed = parse_dsse_envelope(&b64).unwrap();
        assert_eq!(parsed.payload_type, PAYLOAD_TYPE_PACK_V1);
    }
}
