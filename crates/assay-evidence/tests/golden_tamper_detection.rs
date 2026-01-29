//! Golden vector tamper detection tests.
//!
//! These tests prove that the golden vectors actually detect changes -
//! they're not just "green because self-fulfilling".

use assay_evidence::mandate::MANDATE_PAYLOAD_TYPE;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Deterministic test key seed (same as generate_golden_vectors.rs).
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn test_verifying_key() -> VerifyingKey {
    SigningKey::from_bytes(&TEST_KEY_SEED).verifying_key()
}

/// Proves: flipping 1 bit in PAE payload causes signature verification to fail.
#[test]
fn golden_vector_fails_on_one_bit_flip() {
    // Load golden vector data (from intent-basic)
    let expected_pae_b64 = "RFNTRXYxIDM4IGFwcGxpY2F0aW9uL3ZuZC5hc3NheS5tYW5kYXRlK2pzb247dj0xIDM0NSB7ImNvbnN0cmFpbnRzIjp7fSwiY29udGV4dCI6eyJhdWRpZW5jZSI6Im15b3JnL2FwcCIsImlzc3VlciI6ImF1dGgubXlvcmcuY29tIn0sIm1hbmRhdGVfaWQiOiJzaGEyNTY6MTMyNDNlODZhYzgxZGExYTBlNTFmYTcwMzM3MWQyOTFiZTY0MjRkZDNmZTNlN2E5YjM4MGQ5NDk3ZTY4YzdjMCIsIm1hbmRhdGVfa2luZCI6ImludGVudCIsInByaW5jaXBhbCI6eyJtZXRob2QiOiJvaWRjIiwic3ViamVjdCI6InVzZXItMTIzIn0sInNjb3BlIjp7Im9wZXJhdGlvbl9jbGFzcyI6InJlYWQiLCJ0b29scyI6WyJzZWFyY2hfKiJdfSwidmFsaWRpdHkiOnsiaXNzdWVkX2F0IjoiMjAyNi0wMS0yOFQxMDowMDowMFoifX0=";
    let expected_signature_b64 =
        "yNdcG9PJoghOnhL4TYURDFl6ZivyeKqlWfsDqT3qLWhlmJCIuYIFyv3wuR7SsB9nE1Wl7hSw/RHwiNLAGKUXDA==";

    let pk = test_verifying_key();

    // First: verify the original golden vector is valid
    let original_pae = BASE64.decode(expected_pae_b64).unwrap();
    let signature_bytes = BASE64.decode(expected_signature_b64).unwrap();
    let signature = Signature::from_slice(&signature_bytes).unwrap();

    assert!(
        pk.verify(&original_pae, &signature).is_ok(),
        "Original golden vector MUST verify - test setup is broken"
    );

    // Now: flip 1 bit in the PAE payload and verify it FAILS
    let mut tampered_pae = original_pae.clone();
    // Flip bit in the payload section (after the header)
    // PAE format: "DSSEv1 38 application/vnd.assay.mandate+json;v=1 345 {...}"
    // Header is ~60 bytes, so byte 100 is well into the payload
    tampered_pae[100] ^= 0x01;

    let result = pk.verify(&tampered_pae, &signature);
    assert!(
        result.is_err(),
        "Tampered PAE (1-bit flip) MUST fail signature verification"
    );

    println!("✅ 1-bit flip in PAE correctly detected");
}

/// Proves: changing mandate_id produces different hash.
#[test]
fn golden_mandate_id_changes_with_content() {
    let expected_mandate_id =
        "sha256:13243e86ac81da1a0e51fa703371d291be6424dd3fe3e7a9b380d9497e68c7c0";
    let hashable_jcs = r#"{"constraints":{},"context":{"audience":"myorg/app","issuer":"auth.myorg.com"},"mandate_kind":"intent","principal":{"method":"oidc","subject":"user-123"},"scope":{"operation_class":"read","tools":["search_*"]},"validity":{"issued_at":"2026-01-28T10:00:00Z"}}"#;

    // Verify original
    let computed_id = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(hashable_jcs.as_bytes()))
    );
    assert_eq!(
        computed_id, expected_mandate_id,
        "Golden mandate_id must match"
    );

    // Tamper: change "user-123" to "user-124"
    let tampered_jcs = hashable_jcs.replace("user-123", "user-124");
    let tampered_id = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(tampered_jcs.as_bytes()))
    );

    assert_ne!(
        tampered_id, expected_mandate_id,
        "Tampered content MUST produce different mandate_id"
    );

    println!("✅ Content change correctly produces different mandate_id");
    println!("   Original: {}", expected_mandate_id);
    println!("   Tampered: {}", tampered_id);
}

/// Proves: wrong key fails verification.
#[test]
fn golden_wrong_key_fails() {
    let expected_pae_b64 = "RFNTRXYxIDM4IGFwcGxpY2F0aW9uL3ZuZC5hc3NheS5tYW5kYXRlK2pzb247dj0xIDM0NSB7ImNvbnN0cmFpbnRzIjp7fSwiY29udGV4dCI6eyJhdWRpZW5jZSI6Im15b3JnL2FwcCIsImlzc3VlciI6ImF1dGgubXlvcmcuY29tIn0sIm1hbmRhdGVfaWQiOiJzaGEyNTY6MTMyNDNlODZhYzgxZGExYTBlNTFmYTcwMzM3MWQyOTFiZTY0MjRkZDNmZTNlN2E5YjM4MGQ5NDk3ZTY4YzdjMCIsIm1hbmRhdGVfa2luZCI6ImludGVudCIsInByaW5jaXBhbCI6eyJtZXRob2QiOiJvaWRjIiwic3ViamVjdCI6InVzZXItMTIzIn0sInNjb3BlIjp7Im9wZXJhdGlvbl9jbGFzcyI6InJlYWQiLCJ0b29scyI6WyJzZWFyY2hfKiJdfSwidmFsaWRpdHkiOnsiaXNzdWVkX2F0IjoiMjAyNi0wMS0yOFQxMDowMDowMFoifX0=";
    let expected_signature_b64 =
        "yNdcG9PJoghOnhL4TYURDFl6ZivyeKqlWfsDqT3qLWhlmJCIuYIFyv3wuR7SsB9nE1Wl7hSw/RHwiNLAGKUXDA==";

    // Generate a different key
    let wrong_seed: [u8; 32] = [0xFF; 32];
    let wrong_key = SigningKey::from_bytes(&wrong_seed).verifying_key();

    let pae = BASE64.decode(expected_pae_b64).unwrap();
    let signature_bytes = BASE64.decode(expected_signature_b64).unwrap();
    let signature = Signature::from_slice(&signature_bytes).unwrap();

    let result = wrong_key.verify(&pae, &signature);
    assert!(
        result.is_err(),
        "Wrong key MUST fail signature verification"
    );

    println!("✅ Wrong key correctly fails verification");
}

/// Proves: PAE format is exactly as specified.
#[test]
fn golden_pae_format_correct() {
    let expected_pae_b64 = "RFNTRXYxIDM4IGFwcGxpY2F0aW9uL3ZuZC5hc3NheS5tYW5kYXRlK2pzb247dj0xIDM0NSB7ImNvbnN0cmFpbnRzIjp7fSwiY29udGV4dCI6eyJhdWRpZW5jZSI6Im15b3JnL2FwcCIsImlzc3VlciI6ImF1dGgubXlvcmcuY29tIn0sIm1hbmRhdGVfaWQiOiJzaGEyNTY6MTMyNDNlODZhYzgxZGExYTBlNTFmYTcwMzM3MWQyOTFiZTY0MjRkZDNmZTNlN2E5YjM4MGQ5NDk3ZTY4YzdjMCIsIm1hbmRhdGVfa2luZCI6ImludGVudCIsInByaW5jaXBhbCI6eyJtZXRob2QiOiJvaWRjIiwic3ViamVjdCI6InVzZXItMTIzIn0sInNjb3BlIjp7Im9wZXJhdGlvbl9jbGFzcyI6InJlYWQiLCJ0b29scyI6WyJzZWFyY2hfKiJdfSwidmFsaWRpdHkiOnsiaXNzdWVkX2F0IjoiMjAyNi0wMS0yOFQxMDowMDowMFoifX0=";

    let pae = BASE64.decode(expected_pae_b64).unwrap();
    let pae_str = String::from_utf8_lossy(&pae);

    // Verify DSSE PAE format: "DSSEv1 LEN(type) type LEN(payload) payload"
    assert!(
        pae_str.starts_with("DSSEv1 "),
        "PAE must start with 'DSSEv1 '"
    );

    // Parse and verify structure
    let parts: Vec<&str> = pae_str.splitn(5, ' ').collect();
    assert_eq!(parts.len(), 5, "PAE must have 5 space-separated parts");

    let version = parts[0];
    let type_len: usize = parts[1].parse().unwrap();
    let payload_type = parts[2];
    let payload_len: usize = parts[3].parse().unwrap();
    let payload = parts[4];

    assert_eq!(version, "DSSEv1");
    assert_eq!(
        type_len,
        MANDATE_PAYLOAD_TYPE.len(),
        "Type length must match"
    );
    assert_eq!(payload_type, MANDATE_PAYLOAD_TYPE);
    assert_eq!(payload_len, payload.len(), "Payload length must match");
    assert!(payload.starts_with('{'), "Payload must be JSON object");

    println!("✅ PAE format verified:");
    println!("   Version: {}", version);
    println!(
        "   Type length: {} (matches {} bytes)",
        type_len,
        payload_type.len()
    );
    println!(
        "   Payload length: {} (matches {} bytes)",
        payload_len,
        payload.len()
    );
}

/// Proves: signed_payload_digest matches actual digest.
#[test]
fn golden_signed_payload_digest_correct() {
    let signable_jcs = r#"{"constraints":{},"context":{"audience":"myorg/app","issuer":"auth.myorg.com"},"mandate_id":"sha256:13243e86ac81da1a0e51fa703371d291be6424dd3fe3e7a9b380d9497e68c7c0","mandate_kind":"intent","principal":{"method":"oidc","subject":"user-123"},"scope":{"operation_class":"read","tools":["search_*"]},"validity":{"issued_at":"2026-01-28T10:00:00Z"}}"#;
    let expected_digest = "sha256:39098db3ab9530a5735f14cdef309d8f6755f2245a62079d8463a9bba13c470a";

    let computed_digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(signable_jcs.as_bytes()))
    );

    assert_eq!(
        computed_digest, expected_digest,
        "signed_payload_digest must match"
    );

    // Tamper and verify different
    let tampered = signable_jcs.replace("user-123", "attacker");
    let tampered_digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(tampered.as_bytes()))
    );
    assert_ne!(tampered_digest, expected_digest);

    println!("✅ signed_payload_digest verified: {}", expected_digest);
}
