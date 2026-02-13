//! DSSE verification boundary for Step-2 split.
//!
//! Contract target:
//! - envelope parse and signature verification
//! - no allow/skip/unsigned policy decisions
//!
//! Forbidden responsibilities:
//! - deciding unsigned-allow policy
//! - handling skip-signature policy

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier};

use crate::error::RegistryResult;
use crate::trust::TrustStore;
use crate::types::DsseEnvelope;

use super::errors_next;
use super::PAYLOAD_TYPE_PACK_V1;

pub(super) fn build_pae_impl(payload_type: &str, payload: &[u8]) -> Vec<u8> {
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

pub(super) fn verify_dsse_signature_bytes_impl(
    canonical_bytes: &[u8],
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    if envelope.payload_type != PAYLOAD_TYPE_PACK_V1 {
        return Err(errors_next::signature_invalid(format!(
            "payload type mismatch: expected {}, got {}",
            PAYLOAD_TYPE_PACK_V1, envelope.payload_type
        )));
    }

    let payload_bytes = BASE64
        .decode(&envelope.payload)
        .map_err(|e| errors_next::signature_invalid(format!("invalid base64 payload: {}", e)))?;

    if payload_bytes != canonical_bytes {
        return Err(errors_next::digest_mismatch(
            format!("canonical payload ({} bytes)", payload_bytes.len()),
            format!("canonical content ({} bytes)", canonical_bytes.len()),
        ));
    }

    if envelope.signatures.is_empty() {
        return Err(errors_next::signature_invalid("no signatures in envelope"));
    }

    let pae = build_pae_impl(&envelope.payload_type, &payload_bytes);
    let mut last_error = None;
    for sig in &envelope.signatures {
        match verify_single_signature_impl(&pae, &sig.key_id, &sig.signature, trust_store) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e),
        }
    }

    Err(last_error.unwrap_or_else(|| errors_next::signature_invalid("no valid signatures")))
}

pub(super) fn verify_single_signature_impl(
    pae: &[u8],
    key_id: &str,
    signature_b64: &str,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    let key = trust_store.get_key(key_id)?;

    let signature_bytes = BASE64
        .decode(signature_b64)
        .map_err(|e| errors_next::signature_invalid(format!("invalid base64 signature: {}", e)))?;

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| errors_next::signature_invalid(format!("invalid signature bytes: {}", e)))?;

    key.verify(pae, &signature)
        .map_err(|_| errors_next::signature_invalid("ed25519 verification failed"))
}
