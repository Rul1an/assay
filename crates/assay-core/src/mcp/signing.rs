//! Tool signing and verification per SPEC-Tool-Signing-v1.
//!
//! Provides ed25519 signing/verification with DSSE-compatible PAE encoding.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::jcs;

/// Payload type for tool definitions (DSSE-style binding).
pub const PAYLOAD_TYPE_TOOL_V1: &str = "application/vnd.assay.tool+json;v=1";

/// The x-assay-sig field name.
pub const SIG_FIELD: &str = "x-assay-sig";

/// Signature algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureAlgorithm {
    Ed25519,
}

/// The x-assay-sig structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSignature {
    pub version: u8,
    pub algorithm: SignatureAlgorithm,
    pub payload_type: String,
    pub payload_digest: String,
    pub key_id: String,
    pub signature: String,
    pub signed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

/// Result of successful verification.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub key_id: String,
    pub signed_at: DateTime<Utc>,
}

/// Verification errors with exit codes.
#[derive(Debug, Clone, thiserror::Error)]
pub enum VerifyError {
    #[error("tool is not signed")]
    NoSignature,

    #[error("payload type mismatch: expected {expected}, got {got}")]
    PayloadTypeMismatch { expected: String, got: String },

    #[error("signature invalid: {reason}")]
    SignatureInvalid { reason: String },

    #[error("key not trusted: {key_id}")]
    KeyNotTrusted { key_id: String },

    #[error("malformed signature: {reason}")]
    MalformedSignature { reason: String },

    #[error("payload digest mismatch")]
    DigestMismatch,

    #[error("key_id mismatch: signature claims {claimed}, actual {actual}")]
    KeyIdMismatch { claimed: String, actual: String },
}

impl VerifyError {
    /// Exit code for CLI.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NoSignature => 2,
            Self::KeyNotTrusted { .. } => 3,
            Self::SignatureInvalid { .. }
            | Self::PayloadTypeMismatch { .. }
            | Self::DigestMismatch
            | Self::KeyIdMismatch { .. } => 4,
            Self::MalformedSignature { .. } => 1,
        }
    }
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
    use pkcs8::EncodePublicKey;
    let doc = key
        .to_public_key_der()
        .context("failed to encode public key as SPKI DER")?;
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

/// Remove x-assay-sig field from tool JSON.
fn strip_signature(tool: &Value) -> Result<Value> {
    let mut tool = tool.clone();
    if let Some(obj) = tool.as_object_mut() {
        obj.remove(SIG_FIELD);
    }
    Ok(tool)
}

/// Compute payload digest.
fn compute_payload_digest(canonical: &[u8]) -> String {
    let hash = Sha256::digest(canonical);
    format!("sha256:{:x}", hash)
}

/// Sign a tool definition.
///
/// # Arguments
///
/// * `tool` - Tool definition JSON (may or may not have existing signature)
/// * `signing_key` - Ed25519 private key
/// * `embed_pubkey` - If true, include public_key in signature
///
/// # Returns
///
/// Tool definition with x-assay-sig field added.
pub fn sign_tool(tool: &Value, signing_key: &SigningKey, embed_pubkey: bool) -> Result<Value> {
    // 1. Remove existing signature
    let tool_without_sig = strip_signature(tool)?;

    // 2. Canonicalize
    let canonical = jcs::to_vec(&tool_without_sig)?;

    // 3. Build PAE
    let pae = build_pae(PAYLOAD_TYPE_TOOL_V1, &canonical);

    // 4. Sign
    let signature: Signature = signing_key.sign(&pae);

    // 5. Compute digests
    let payload_digest = compute_payload_digest(&canonical);
    let verifying_key = signing_key.verifying_key();
    let key_id = compute_key_id_from_verifying_key(&verifying_key)?;

    // 6. Build x-assay-sig
    let sig = ToolSignature {
        version: 1,
        algorithm: SignatureAlgorithm::Ed25519,
        payload_type: PAYLOAD_TYPE_TOOL_V1.to_string(),
        payload_digest,
        key_id,
        signature: BASE64.encode(signature.to_bytes()),
        signed_at: Utc::now(),
        public_key: if embed_pubkey {
            let spki = key_to_spki_der(&verifying_key)?;
            Some(BASE64.encode(&spki))
        } else {
            None
        },
    };

    // 7. Add to tool
    let mut result = tool_without_sig;
    if let Some(obj) = result.as_object_mut() {
        obj.insert(SIG_FIELD.to_string(), serde_json::to_value(&sig)?);
    } else {
        bail!("tool must be a JSON object");
    }

    Ok(result)
}

/// Verify a signed tool definition.
///
/// # Arguments
///
/// * `tool` - Signed tool definition JSON
/// * `trusted_key` - Public key to verify against
///
/// # Returns
///
/// `VerifyResult` on success, `VerifyError` on failure.
pub fn verify_tool(tool: &Value, trusted_key: &VerifyingKey) -> Result<VerifyResult, VerifyError> {
    // 1. Extract signature
    let sig_value = tool.get(SIG_FIELD).ok_or(VerifyError::NoSignature)?;

    let sig: ToolSignature =
        serde_json::from_value(sig_value.clone()).map_err(|e| VerifyError::MalformedSignature {
            reason: e.to_string(),
        })?;

    // 2. Validate version and algorithm
    if sig.version != 1 {
        return Err(VerifyError::MalformedSignature {
            reason: format!("unsupported version: {}", sig.version),
        });
    }
    if sig.algorithm != SignatureAlgorithm::Ed25519 {
        return Err(VerifyError::MalformedSignature {
            reason: format!("unsupported algorithm: {:?}", sig.algorithm),
        });
    }

    // 3. Validate payload_type
    if sig.payload_type != PAYLOAD_TYPE_TOOL_V1 {
        return Err(VerifyError::PayloadTypeMismatch {
            expected: PAYLOAD_TYPE_TOOL_V1.to_string(),
            got: sig.payload_type,
        });
    }

    // 4. Strip signature and canonicalize
    let tool_without_sig = strip_signature(tool).map_err(|e| VerifyError::MalformedSignature {
        reason: e.to_string(),
    })?;
    let canonical =
        jcs::to_vec(&tool_without_sig).map_err(|e| VerifyError::MalformedSignature {
            reason: e.to_string(),
        })?;

    // 5. Verify payload digest
    let computed_digest = compute_payload_digest(&canonical);
    if sig.payload_digest != computed_digest {
        return Err(VerifyError::DigestMismatch);
    }

    // 6. Build PAE and verify signature
    let pae = build_pae(&sig.payload_type, &canonical);
    let signature_bytes =
        BASE64
            .decode(&sig.signature)
            .map_err(|e| VerifyError::MalformedSignature {
                reason: format!("invalid base64 signature: {}", e),
            })?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|e| VerifyError::MalformedSignature {
            reason: format!("invalid signature bytes: {}", e),
        })?;

    trusted_key
        .verify(&pae, &signature)
        .map_err(|_| VerifyError::SignatureInvalid {
            reason: "ed25519 verification failed".to_string(),
        })?;

    // 7. Verify key_id matches
    let actual_key_id = compute_key_id_from_verifying_key(trusted_key).map_err(|e| {
        VerifyError::MalformedSignature {
            reason: e.to_string(),
        }
    })?;
    if sig.key_id != actual_key_id {
        return Err(VerifyError::KeyIdMismatch {
            claimed: sig.key_id,
            actual: actual_key_id,
        });
    }

    Ok(VerifyResult {
        key_id: sig.key_id,
        signed_at: sig.signed_at,
    })
}

/// Extract signature from a tool (if present).
pub fn extract_signature(tool: &Value) -> Option<ToolSignature> {
    tool.get(SIG_FIELD)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Check if a tool is signed.
pub fn is_signed(tool: &Value) -> bool {
    tool.get(SIG_FIELD).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn generate_keypair() -> SigningKey {
        SigningKey::generate(&mut rand::thread_rng())
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let key = generate_keypair();
        let tool = json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {"type": "object"}
        });

        let signed = sign_tool(&tool, &key, false).unwrap();
        assert!(is_signed(&signed));

        let result = verify_tool(&signed, &key.verifying_key()).unwrap();
        assert!(result.key_id.starts_with("sha256:"));
    }

    #[test]
    fn test_tamper_detection() {
        let key = generate_keypair();
        let tool = json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {"type": "object"}
        });

        let mut signed = sign_tool(&tool, &key, false).unwrap();

        // Tamper with the tool
        signed["description"] = json!("Malicious description");

        let result = verify_tool(&signed, &key.verifying_key());
        assert!(matches!(result, Err(VerifyError::DigestMismatch)));
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_keypair();
        let key2 = generate_keypair();
        let tool = json!({
            "name": "test_tool",
            "description": "Test",
            "inputSchema": {}
        });

        let signed = sign_tool(&tool, &key1, false).unwrap();
        let result = verify_tool(&signed, &key2.verifying_key());

        // Should fail with either SignatureInvalid or KeyIdMismatch
        assert!(matches!(
            result,
            Err(VerifyError::SignatureInvalid { .. }) | Err(VerifyError::KeyIdMismatch { .. })
        ));
    }

    #[test]
    fn test_unsigned_tool() {
        let key = generate_keypair();
        let tool = json!({"name": "unsigned"});

        let result = verify_tool(&tool, &key.verifying_key());
        assert!(matches!(result, Err(VerifyError::NoSignature)));
    }

    #[test]
    fn test_embed_pubkey() {
        let key = generate_keypair();
        let tool = json!({"name": "test", "description": "test", "inputSchema": {}});

        let signed = sign_tool(&tool, &key, true).unwrap();
        let sig = extract_signature(&signed).unwrap();

        assert!(sig.public_key.is_some());
    }

    #[test]
    fn test_key_id_computation() {
        let key = generate_keypair();
        let key_id = compute_key_id_from_verifying_key(&key.verifying_key()).unwrap();

        assert!(key_id.starts_with("sha256:"));
        assert_eq!(key_id.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_pae_format() {
        let pae = build_pae("application/json", b"test");

        // "DSSEv1 16 application/json 4 test"
        let expected = b"DSSEv1 16 application/json 4 test";
        assert_eq!(pae, expected);
    }

    #[test]
    fn test_canonicalization_stability() {
        let key = generate_keypair();

        // Same tool, different JSON formatting
        let tool1 =
            json!({"name": "test", "description": "desc", "inputSchema": {"type": "object"}});
        let tool2 =
            json!({"inputSchema": {"type": "object"}, "name": "test", "description": "desc"});

        let signed1 = sign_tool(&tool1, &key, false).unwrap();
        let signed2 = sign_tool(&tool2, &key, false).unwrap();

        // Both should have the same payload_digest
        let sig1 = extract_signature(&signed1).unwrap();
        let sig2 = extract_signature(&signed2).unwrap();

        assert_eq!(sig1.payload_digest, sig2.payload_digest);
    }

    #[test]
    fn test_exit_codes() {
        assert_eq!(VerifyError::NoSignature.exit_code(), 2);
        assert_eq!(
            VerifyError::KeyNotTrusted { key_id: "x".into() }.exit_code(),
            3
        );
        assert_eq!(
            VerifyError::SignatureInvalid { reason: "x".into() }.exit_code(),
            4
        );
        assert_eq!(
            VerifyError::MalformedSignature { reason: "x".into() }.exit_code(),
            1
        );
    }
}
