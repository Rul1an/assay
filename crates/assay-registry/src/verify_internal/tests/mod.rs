use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;

use crate::error::{RegistryError, RegistryResult};
use crate::trust::TrustStore;
use crate::types::{DsseEnvelope, FetchResult};
use crate::verify::{
    compute_digest, compute_digest_strict, compute_key_id, compute_key_id_from_key, verify_digest,
    verify_pack, VerifyOptions, PAYLOAD_TYPE_PACK_V1,
};

fn canonicalize_for_dsse(content: &str) -> RegistryResult<Vec<u8>> {
    super::digest::canonicalize_for_dsse_impl(content)
}

fn parse_dsse_envelope(b64: &str) -> RegistryResult<DsseEnvelope> {
    super::wire::parse_dsse_envelope_impl(b64)
}

fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    super::dsse::build_pae_impl(payload_type, payload)
}

fn verify_dsse_signature_bytes(
    canonical_bytes: &[u8],
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    super::dsse::verify_dsse_signature_bytes_impl(canonical_bytes, envelope, trust_store)
}

fn generate_keypair() -> SigningKey {
    SigningKey::generate(&mut rand::thread_rng())
}

// Legacy compatibility helper kept test-local: production path verifies
// canonical bytes via verify_dsse_signature_bytes; callers canonicalize.
fn verify_dsse_signature_legacy_for_tests(
    content: &str,
    envelope: &DsseEnvelope,
    trust_store: &TrustStore,
) -> RegistryResult<()> {
    let canonical_bytes = canonicalize_for_dsse(content)?;
    verify_dsse_signature_bytes(&canonical_bytes, envelope, trust_store)
}

mod digest;
mod dsse;
mod failures;
mod provenance;

fn keypair_from_seed(seed: [u8; 32]) -> SigningKey {
    SigningKey::from_bytes(&seed)
}

/// Helper to create a DSSE envelope with real signature.
fn create_signed_envelope(signing_key: &SigningKey, content: &str) -> (DsseEnvelope, String) {
    use ed25519_dalek::Signer;
    use pkcs8::EncodePublicKey;

    // Canonicalize content
    let canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();

    // Compute key ID
    let verifying_key = signing_key.verifying_key();
    let spki_der = verifying_key.to_public_key_der().unwrap();
    let key_id = compute_key_id(spki_der.as_bytes());

    // Build PAE and sign
    let payload_b64 = BASE64.encode(&canonical);
    let pae = build_pae(PAYLOAD_TYPE_PACK_V1, &canonical);
    let signature = signing_key.sign(&pae);

    let envelope = DsseEnvelope {
        payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
        payload: payload_b64,
        signatures: vec![crate::types::DsseSignature {
            key_id: key_id.clone(),
            signature: BASE64.encode(signature.to_bytes()),
        }],
    };

    (envelope, key_id)
}

fn make_fetch_result(
    content: &str,
    digest: Option<String>,
    signature: Option<String>,
    key_id: Option<String>,
) -> FetchResult {
    FetchResult {
        content: content.to_string(),
        headers: crate::types::PackHeaders {
            digest,
            signature,
            key_id,
            etag: None,
            cache_control: None,
            content_length: None,
        },
        computed_digest: compute_digest(content),
    }
}
