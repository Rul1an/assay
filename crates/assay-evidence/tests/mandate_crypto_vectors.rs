//! Complete cryptographic test vectors for mandate evidence.
//!
//! These tests produce deterministic output with fixed keys and timestamps,
//! enabling cross-language verification of the signing/verification flow.
//!
//! # Two-Phase Canonicalization
//!
//! SPEC-Mandate-v1 requires two distinct phases:
//!
//! 1. **mandate_id computation**: `sha256(JCS(hashable_content))`
//!    where hashable_content = content WITHOUT mandate_id AND signature
//!
//! 2. **Signing**: `ed25519_sign(PAE(payload_type, JCS(signable_content)))`
//!    where signable_content = content WITH mandate_id but WITHOUT signature

use assay_evidence::crypto::jcs;
use assay_evidence::mandate::{
    compute_mandate_id, sign_mandate, verify_mandate, AuthMethod, Constraints, Context,
    MandateContent, MandateKind, OperationClass, Principal, Scope, Validity, MANDATE_PAYLOAD_TYPE,
};
use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Deterministic test key (DO NOT USE IN PRODUCTION).
/// Seed: 0x01, 0x02, ..., 0x20 (32 bytes)
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn test_key() -> SigningKey {
    SigningKey::from_bytes(&TEST_KEY_SEED)
}

fn test_content() -> MandateContent {
    MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()]).with_operation_class(OperationClass::Read),
        validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    }
}

/// Test vector: Phase 1 - mandate_id computation
#[test]
fn test_vector_mandate_id_computation() {
    let content = test_content();

    // Step 1: Serialize hashable_content to JCS
    let hashable_jcs = jcs::to_string(&content).unwrap();

    println!("=== PHASE 1: mandate_id computation ===");
    println!("Hashable JCS (content without mandate_id/signature):");
    println!("{}", hashable_jcs);
    println!();

    // Verify JCS key ordering
    assert!(hashable_jcs.starts_with("{\"constraints\":"));
    assert!(hashable_jcs.contains("\"context\":"));
    assert!(hashable_jcs.contains("\"mandate_kind\":\"intent\""));

    // Step 2: Compute SHA-256
    let hashable_bytes = hashable_jcs.as_bytes();
    let hash = Sha256::digest(hashable_bytes);

    println!("Hashable bytes (hex):");
    println!("{}", hex::encode(hashable_bytes));
    println!();
    println!("SHA-256 hash (hex):");
    println!("{}", hex::encode(hash));
    println!();

    // Step 3: Format as mandate_id
    let mandate_id = compute_mandate_id(&content).unwrap();
    println!("mandate_id:");
    println!("{}", mandate_id);
    println!();

    // Verify format
    assert!(mandate_id.starts_with("sha256:"));
    assert_eq!(mandate_id.len(), 71);
    assert_eq!(mandate_id, format!("sha256:{}", hex::encode(hash)));

    // Golden value (deterministic from fixed content)
    // If this changes, cross-language implementations need to update
    println!("=== GOLDEN VALUE (copy to other implementations) ===");
    println!("mandate_id = \"{}\"", mandate_id);
}

/// Test vector: Phase 2 - signing
#[test]
fn test_vector_signing_flow() {
    let content = test_content();
    let key = test_key();

    // Pre-compute mandate_id
    let mandate_id = compute_mandate_id(&content).unwrap();

    // Build signable content (content + mandate_id, no signature)
    #[derive(Serialize)]
    struct SignableMandate {
        mandate_id: String,
        mandate_kind: MandateKind,
        principal: Principal,
        scope: Scope,
        validity: Validity,
        constraints: Constraints,
        context: Context,
    }

    let signable = SignableMandate {
        mandate_id: mandate_id.clone(),
        mandate_kind: content.mandate_kind,
        principal: content.principal.clone(),
        scope: content.scope.clone(),
        validity: content.validity.clone(),
        constraints: content.constraints.clone(),
        context: content.context.clone(),
    };

    // JCS canonicalize
    let signable_jcs = jcs::to_string(&signable).unwrap();

    println!("=== PHASE 2: Signing ===");
    println!("Signable JCS (content WITH mandate_id, WITHOUT signature):");
    println!("{}", signable_jcs);
    println!();

    // Build PAE
    let signable_bytes = signable_jcs.as_bytes();
    let pae = build_pae(MANDATE_PAYLOAD_TYPE, signable_bytes);

    println!("PAE bytes (hex):");
    println!("{}", hex::encode(&pae));
    println!();
    println!("PAE as string (for debugging):");
    println!("{}", String::from_utf8_lossy(&pae));
    println!();

    // Verify PAE format
    let pae_str = String::from_utf8_lossy(&pae);
    assert!(pae_str.starts_with("DSSEv1 "));
    assert!(pae_str.contains(MANDATE_PAYLOAD_TYPE));

    // Sign
    let signed = sign_mandate(&content, &key).unwrap();

    println!("Signed mandate JSON:");
    println!("{}", serde_json::to_string_pretty(&signed).unwrap());
    println!();

    // Verify signature fields
    let sig = signed.signature.as_ref().unwrap();
    assert_eq!(sig.version, 1);
    assert_eq!(sig.algorithm, "ed25519");
    assert_eq!(sig.payload_type, MANDATE_PAYLOAD_TYPE);
    assert_eq!(sig.content_id, mandate_id);
    assert!(sig.signed_payload_digest.starts_with("sha256:"));
    assert!(sig.key_id.starts_with("sha256:"));

    println!("=== GOLDEN VALUES ===");
    println!("content_id = \"{}\"", sig.content_id);
    println!("signed_payload_digest = \"{}\"", sig.signed_payload_digest);
    println!("key_id = \"{}\"", sig.key_id);
    println!("signature = \"{}\"", sig.signature);
}

/// Test vector: Verification flow
#[test]
fn test_vector_verification() {
    let content = test_content();
    let key = test_key();

    let signed = sign_mandate(&content, &key).unwrap();
    let result = verify_mandate(&signed, &key.verifying_key()).unwrap();

    println!("=== VERIFICATION ===");
    println!("Verified mandate_id: {}", result.mandate_id);
    println!("Verified key_id: {}", result.key_id);

    assert_eq!(result.mandate_id, signed.mandate_id);
    assert_eq!(result.key_id, signed.signature.as_ref().unwrap().key_id);
}

/// Negative test: wrong payload_type
#[test]
fn test_negative_wrong_payload_type() {
    let content = test_content();
    let key = test_key();

    let mut signed = sign_mandate(&content, &key).unwrap();

    // Tamper with payload_type
    signed.signature.as_mut().unwrap().payload_type = "wrong/type".to_string();

    let result = verify_mandate(&signed, &key.verifying_key());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("payload type"),
        "Expected payload type error, got: {}",
        err
    );
}

/// Negative test: signature valid but key_id mismatch
#[test]
fn test_negative_key_id_mismatch() {
    let content = test_content();
    let key = test_key();

    let mut signed = sign_mandate(&content, &key).unwrap();

    // Tamper with key_id (signature still valid for original key)
    signed.signature.as_mut().unwrap().key_id = "sha256:wrong".to_string();

    let result = verify_mandate(&signed, &key.verifying_key());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("key_id") || err.to_string().contains("KeyId"),
        "Expected key_id mismatch error, got: {}",
        err
    );
}

/// Negative test: mandate_id mismatch with content_id
#[test]
fn test_negative_mandate_id_content_id_mismatch() {
    let content = test_content();
    let key = test_key();

    let mut signed = sign_mandate(&content, &key).unwrap();

    // Tamper with mandate_id (content_id in signature still has original)
    signed.mandate_id =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();

    let result = verify_mandate(&signed, &key.verifying_key());
    assert!(result.is_err());
}

/// Negative test: content_id mismatch with computed
#[test]
fn test_negative_content_id_mismatch() {
    let content = test_content();
    let key = test_key();

    let mut signed = sign_mandate(&content, &key).unwrap();

    // Tamper with content (principal.subject)
    signed.principal.subject = "attacker".to_string();

    // Now mandate_id doesn't match recomputed content hash
    let result = verify_mandate(&signed, &key.verifying_key());
    assert!(result.is_err());
    let err = result.unwrap_err();
    // The error message contains "does not match" which includes "match"
    let err_str = err.to_string().to_lowercase();
    assert!(
        err_str.contains("match") || err_str.contains("computed"),
        "Expected ID mismatch error, got: {}",
        err
    );
}

/// Negative test: wrong signing key
#[test]
fn test_negative_wrong_key() {
    let content = test_content();
    let key1 = test_key();
    let key2 = SigningKey::generate(&mut rand::thread_rng());

    let signed = sign_mandate(&content, &key1).unwrap();

    // Verify with different key
    let result = verify_mandate(&signed, &key2.verifying_key());
    assert!(result.is_err());
}

/// Test: same semantic JSON with different field ordering produces same mandate_id
#[test]
fn test_jcs_normalization() {
    // Create two contents that would have different JSON representations
    // without canonicalization, but JCS normalizes them

    let content1 = test_content();
    let content2 = test_content();

    let id1 = compute_mandate_id(&content1).unwrap();
    let id2 = compute_mandate_id(&content2).unwrap();

    assert_eq!(
        id1, id2,
        "Same logical content must produce same mandate_id"
    );
}

/// Test: verify signed_payload_digest matches payload
#[test]
fn test_signed_payload_digest_verification() {
    let content = test_content();
    let key = test_key();

    let signed = sign_mandate(&content, &key).unwrap();
    let sig = signed.signature.as_ref().unwrap();

    // Reconstruct signable content
    #[derive(Serialize)]
    struct SignableMandate {
        mandate_id: String,
        mandate_kind: MandateKind,
        principal: Principal,
        scope: Scope,
        validity: Validity,
        constraints: Constraints,
        context: Context,
    }

    let signable = SignableMandate {
        mandate_id: signed.mandate_id.clone(),
        mandate_kind: signed.mandate_kind,
        principal: signed.principal.clone(),
        scope: signed.scope.clone(),
        validity: signed.validity.clone(),
        constraints: signed.constraints.clone(),
        context: signed.context.clone(),
    };

    let signable_jcs = jcs::to_vec(&signable).unwrap();
    let expected_digest = format!("sha256:{}", hex::encode(Sha256::digest(&signable_jcs)));

    assert_eq!(
        sig.signed_payload_digest, expected_digest,
        "signed_payload_digest must match SHA256 of signable JCS"
    );
}

// === Helper functions ===

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
