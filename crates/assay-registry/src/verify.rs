//! Pack verification (digest and signature).
//!
//! Implements verification per SPEC-Pack-Registry-v1:
//! - Digest verification (SHA-256 of JCS-canonical content)
//! - DSSE signature verification (Ed25519 over PAE)

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::canonicalize::{compute_canonical_digest, CanonicalizeError};
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
///
/// Uses canonical JCS digest per SPEC §6.2.
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

/// Compute canonical digest of content per SPEC §6.2.
///
/// Process:
/// 1. Parse YAML with strict validation (§6.1)
/// 2. Convert to JSON
/// 3. Serialize to JCS (RFC 8785)
/// 4. SHA-256 hash
///
/// For content that may not be valid YAML, falls back to raw SHA-256.
pub fn compute_digest(content: &str) -> String {
    // Try canonical digest first
    match compute_canonical_digest(content) {
        Ok(digest) => digest,
        Err(e) => {
            // Log warning and fall back to raw digest for non-YAML content
            tracing::warn!(
                error = %e,
                "canonical digest failed, falling back to raw digest"
            );
            // Inline raw digest to avoid deprecation warning
            let hash = Sha256::digest(content.as_bytes());
            format!("sha256:{:x}", hash)
        }
    }
}

/// Compute canonical digest, returning error on invalid YAML.
///
/// Use this when you need strict validation.
pub fn compute_digest_strict(content: &str) -> Result<String, CanonicalizeError> {
    compute_canonical_digest(content)
}

/// Compute raw SHA-256 digest of content bytes.
///
/// **Deprecated**: Use `compute_digest` for canonical JCS digest per SPEC §6.2.
/// This function is only for backward compatibility with pre-v1.0.2 digests.
#[deprecated(since = "2.11.0", note = "use compute_digest for canonical JCS digest")]
#[allow(dead_code)]
pub fn compute_digest_raw(content: &str) -> String {
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
    fn test_compute_digest_canonical() {
        // Valid YAML should use canonical JCS digest
        let content = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);

        // Verify it's the canonical digest (JCS sorts keys)
        let strict = compute_digest_strict(content).unwrap();
        assert_eq!(digest, strict);
    }

    #[test]
    fn test_compute_digest_golden_vector() {
        // Golden vector from SPEC review
        let content = "name: eu-ai-act-baseline\nversion: \"1.0.0\"\nkind: compliance";
        let digest = compute_digest(content);

        // This is the JCS canonical digest
        assert_eq!(
            digest,
            "sha256:f47d932cdad4bde369ed0a7cf26fdcf4077777296346c4102d9017edbc62a070"
        );
    }

    #[test]
    fn test_compute_digest_key_ordering() {
        // Key order in YAML shouldn't matter for canonical digest
        let yaml1 = "z: 1\na: 2";
        let yaml2 = "a: 2\nz: 1";

        let digest1 = compute_digest(yaml1);
        let digest2 = compute_digest(yaml2);

        assert_eq!(digest1, digest2);
    }

    #[test]
    #[allow(deprecated)]
    fn test_compute_digest_raw_differs() {
        // Raw digest differs from canonical
        let content = "name: eu-ai-act-baseline\nversion: \"1.0.0\"\nkind: compliance";

        let canonical = compute_digest(content);
        let raw = compute_digest_raw(content);

        // They should be different!
        assert_ne!(canonical, raw);

        // Raw is what we had before (review golden vector)
        assert_eq!(
            raw,
            "sha256:5a9a6b1e95e8c1d36779b87212835c9bfa9cae5d98cb9c75fb8c478750e5e200"
        );
    }

    #[test]
    fn test_verify_digest_success() {
        let content = "name: test\nversion: \"1.0.0\"";
        let expected = compute_digest(content);
        assert!(verify_digest(content, &expected).is_ok());
    }

    #[test]
    fn test_verify_digest_mismatch() {
        let content = "name: test\nversion: \"1.0.0\"";
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

    // ==================== Header Size Regression Tests ====================

    #[test]
    fn test_dsse_envelope_size_small_pack() {
        // Small pack (< 1KB) should fit in header
        let content = "name: small-pack\nversion: \"1.0.0\"\nrules: []";
        let canonical = crate::canonicalize::to_canonical_jcs_bytes(
            &crate::canonicalize::parse_yaml_strict(content).unwrap(),
        )
        .unwrap();

        let envelope = DsseEnvelope {
            payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
            payload: BASE64.encode(&canonical),
            signatures: vec![crate::types::DsseSignature {
                key_id: "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd"
                    .to_string(),
                signature: BASE64.encode([0u8; 64]), // Ed25519 signature
            }],
        };

        let json = serde_json::to_vec(&envelope).unwrap();
        let header_value = BASE64.encode(&json);

        // Small pack envelope should be < 1KB (comfortably within 8KB header limit)
        assert!(
            header_value.len() < 1024,
            "Small pack DSSE envelope should be < 1KB, got {} bytes",
            header_value.len()
        );
    }

    #[test]
    fn test_dsse_envelope_size_medium_pack() {
        // Medium pack (~4KB canonical) - this is where header limits become risky
        let mut content = String::from("name: medium-pack\nversion: \"1.0.0\"\nrules:\n");
        for i in 0..100 {
            content.push_str(&format!(
                "  - name: rule_{}\n    pattern: \"test_pattern_{}\"\n",
                i, i
            ));
        }

        let canonical = crate::canonicalize::to_canonical_jcs_bytes(
            &crate::canonicalize::parse_yaml_strict(&content).unwrap(),
        )
        .unwrap();

        let envelope = DsseEnvelope {
            payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
            payload: BASE64.encode(&canonical),
            signatures: vec![crate::types::DsseSignature {
                key_id: "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd"
                    .to_string(),
                signature: BASE64.encode([0u8; 64]),
            }],
        };

        let json = serde_json::to_vec(&envelope).unwrap();
        let header_value = BASE64.encode(&json);

        // Document the size - this helps understand when sidecar is needed
        println!(
            "Medium pack: canonical={} bytes, envelope={} bytes, header={} bytes",
            canonical.len(),
            json.len(),
            header_value.len()
        );

        // If over 8KB, sidecar endpoint MUST be used
        if header_value.len() > 8192 {
            println!("WARNING: Pack exceeds 8KB header limit - use sidecar endpoint");
        }
    }

    #[test]
    fn test_header_size_limit_constant() {
        // Document the recommended header size limit
        const RECOMMENDED_HEADER_LIMIT: usize = 8192; // 8KB

        // Most reverse proxies/CDNs use 8KB as default
        // nginx: proxy_buffer_size (default 4KB, commonly set to 8KB)
        // AWS ALB: header limit 16KB
        // Cloudflare: header limit ~16KB
        // Conservative choice: 8KB

        assert_eq!(RECOMMENDED_HEADER_LIMIT, 8192);
    }
}
