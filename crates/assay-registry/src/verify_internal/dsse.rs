//! DSSE crypto boundary for verify split.
//!
//! Contract target:
//! - PAE building and signature verification only
//! - no allow/deny policy decisions

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier};

use crate::error::{RegistryError, RegistryResult};
use crate::trust::TrustStore;
use crate::types::DsseEnvelope;

use super::super::PAYLOAD_TYPE_PACK_V1;
use super::digest::canonicalize_for_dsse_impl;

/// Delegates to [`assay_common::dsse::build_pae`], the workspace's one PAE.
pub(crate) fn build_pae_impl(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    assay_common::dsse::build_pae(payload_type, payload)
}

pub(crate) fn verify_dsse_signature_bytes_impl(
    canonical_bytes: &[u8],
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    if envelope.payload_type != PAYLOAD_TYPE_PACK_V1 {
        return Err(RegistryError::SignatureInvalid {
            reason: format!(
                "payload type mismatch: expected {}, got {}",
                PAYLOAD_TYPE_PACK_V1, envelope.payload_type
            ),
        });
    }

    // Bounded before the allocation; see `dsse_limits`.
    let payload_bytes = match crate::dsse_limits::decode_bounded(envelope.payload.as_bytes()) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Err(RegistryError::SignatureInvalid {
                reason: "invalid base64 payload".to_string(),
            })
        }
        Err(too_large) => {
            return Err(RegistryError::SignatureInvalid {
                reason: too_large.reason(),
            })
        }
    };

    if payload_bytes != canonical_bytes {
        return Err(RegistryError::DigestMismatch {
            name: "pack".to_string(),
            version: "unknown".to_string(),
            expected: format!("canonical payload ({} bytes)", payload_bytes.len()),
            actual: format!("canonical content ({} bytes)", canonical_bytes.len()),
        });
    }

    if envelope.signatures.is_empty() {
        return Err(RegistryError::SignatureInvalid {
            reason: "no signatures in envelope".to_string(),
        });
    }

    let pae = build_pae_impl(&envelope.payload_type, &payload_bytes);

    let mut last_error = None;
    for sig in &envelope.signatures {
        match verify_single_signature_impl(&pae, &sig.key_id, &sig.signature, trust_store) {
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

pub(crate) fn verify_dsse_signature_impl(
    content: &str,
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    let canonical_bytes = canonicalize_for_dsse_impl(content)?;
    verify_dsse_signature_bytes_impl(&canonical_bytes, envelope, trust_store)
}

pub(crate) fn verify_single_signature_impl(
    pae: &[u8],
    key_id: &str,
    signature_b64: &str,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    let key = trust_store.get_key(key_id)?;

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

    key.verify(pae, &signature)
        .map_err(|_| RegistryError::SignatureInvalid {
            reason: "ed25519 verification failed".to_string(),
        })
}
