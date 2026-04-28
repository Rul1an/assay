use super::*;

#[test]
fn test_dsse_valid_signature_real_ed25519() {
    // SPEC §6.3.4: Valid DSSE with real Ed25519 signature
    // Use deterministic seed for reproducibility
    let seed: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];
    let signing_key = keypair_from_seed(seed);
    let content = "name: test-pack\nversion: \"1.0.0\"\nrules: []";

    let (envelope, key_id) = create_signed_envelope(&signing_key, content);

    // Build trust store with this key
    use ed25519_dalek::pkcs8::EncodePublicKey;
    let verifying_key = signing_key.verifying_key();
    let spki_der = verifying_key.to_public_key_der().unwrap();
    let trusted_key = crate::types::TrustedKey {
        key_id: key_id.clone(),
        algorithm: "Ed25519".to_string(),
        public_key: BASE64.encode(spki_der.as_bytes()),
        description: Some("Test key".to_string()),
        added_at: None,
        expires_at: None,
        revoked: false,
    };

    let trust_store = TrustStore::new();
    // Use blocking runtime for sync test
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(trust_store.add_pinned_key(&trusted_key))
        .unwrap();

    // Verify signature
    let canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();
    let content_str = String::from_utf8(canonical.clone()).unwrap();

    let result = verify_dsse_signature_legacy_for_tests(&content_str, &envelope, &trust_store);
    assert!(result.is_ok(), "DSSE signature should verify: {:?}", result);
}

#[test]
fn test_dsse_payload_mismatch() {
    // SPEC §6.3.3: Payload in envelope must match content
    // Envelope contains "original", verify against "tampered"
    let seed: [u8; 32] = [0x42; 32];
    let signing_key = keypair_from_seed(seed);

    // Create envelope for original content
    let original_content = "name: original\nversion: \"1.0.0\"";
    let (envelope, key_id) = create_signed_envelope(&signing_key, original_content);

    // Build trust store
    use ed25519_dalek::pkcs8::EncodePublicKey;
    let verifying_key = signing_key.verifying_key();
    let spki_der = verifying_key.to_public_key_der().unwrap();
    let trusted_key = crate::types::TrustedKey {
        key_id,
        algorithm: "Ed25519".to_string(),
        public_key: BASE64.encode(spki_der.as_bytes()),
        description: None,
        added_at: None,
        expires_at: None,
        revoked: false,
    };

    let trust_store = TrustStore::new();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(trust_store.add_pinned_key(&trusted_key))
        .unwrap();

    // Try to verify with DIFFERENT content (attack scenario)
    let tampered_content = "name: tampered\nversion: \"1.0.0\"";
    let tampered_canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(tampered_content).unwrap(),
    )
    .unwrap();
    let tampered_str = String::from_utf8(tampered_canonical).unwrap();

    let result = verify_dsse_signature_legacy_for_tests(&tampered_str, &envelope, &trust_store);
    assert!(
        matches!(result, Err(RegistryError::DigestMismatch { .. })),
        "Should return DigestMismatch for payload != content: {:?}",
        result
    );
}

#[test]
fn test_dsse_untrusted_key_rejected() {
    // SPEC §6.4.4: Unknown keys MUST be rejected for commercial packs
    let seed: [u8; 32] = [0x55; 32];
    let signing_key = keypair_from_seed(seed);
    let content = "name: commercial-pack\nversion: \"1.0.0\"";

    let (envelope, _key_id) = create_signed_envelope(&signing_key, content);

    // Empty trust store - key is NOT trusted
    let trust_store = TrustStore::new();

    let canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();
    let content_str = String::from_utf8(canonical).unwrap();

    let result = verify_dsse_signature_legacy_for_tests(&content_str, &envelope, &trust_store);
    assert!(
        matches!(result, Err(RegistryError::KeyNotTrusted { .. })),
        "Should return KeyNotTrusted for unknown key: {:?}",
        result
    );
}

#[test]
fn test_dsse_wrong_payload_type_rejected() {
    // SPEC §6.3.2: Payload type must match expected type
    let envelope = DsseEnvelope {
        payload_type: "application/json".to_string(), // Wrong type!
        payload: BASE64.encode(b"test"),
        signatures: vec![],
    };

    let trust_store = TrustStore::new();
    let result = verify_dsse_signature_legacy_for_tests("test", &envelope, &trust_store);

    assert!(
        matches!(result, Err(RegistryError::SignatureInvalid { .. })),
        "Should reject wrong payload type: {:?}",
        result
    );
}

#[test]
fn test_dsse_empty_signatures_rejected() {
    // SPEC §6.3: At least one signature required
    let content = "name: test\nversion: \"1.0.0\"";
    let canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();

    let envelope = DsseEnvelope {
        payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
        payload: BASE64.encode(&canonical),
        signatures: vec![], // No signatures!
    };

    let trust_store = TrustStore::new();
    let content_str = String::from_utf8(canonical).unwrap();
    let result = verify_dsse_signature_legacy_for_tests(&content_str, &envelope, &trust_store);

    assert!(
        matches!(result, Err(RegistryError::SignatureInvalid { .. })),
        "Should reject empty signatures: {:?}",
        result
    );
}

#[test]
fn test_dsse_invalid_signature_rejected() {
    // Invalid signature bytes should be rejected
    let seed: [u8; 32] = [0x77; 32];
    let signing_key = keypair_from_seed(seed);
    let content = "name: test\nversion: \"1.0.0\"";

    let canonical = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();

    // Compute key ID
    use ed25519_dalek::pkcs8::EncodePublicKey;
    let verifying_key = signing_key.verifying_key();
    let spki_der = verifying_key.to_public_key_der().unwrap();
    let key_id = compute_key_id(spki_der.as_bytes());

    // Create envelope with INVALID signature (all zeros)
    let envelope = DsseEnvelope {
        payload_type: PAYLOAD_TYPE_PACK_V1.to_string(),
        payload: BASE64.encode(&canonical),
        signatures: vec![crate::types::DsseSignature {
            key_id: key_id.clone(),
            signature: BASE64.encode([0u8; 64]), // Invalid signature!
        }],
    };

    // Add key to trust store
    let trusted_key = crate::types::TrustedKey {
        key_id,
        algorithm: "Ed25519".to_string(),
        public_key: BASE64.encode(spki_der.as_bytes()),
        description: None,
        added_at: None,
        expires_at: None,
        revoked: false,
    };

    let trust_store = TrustStore::new();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(trust_store.add_pinned_key(&trusted_key))
        .unwrap();

    let content_str = String::from_utf8(canonical).unwrap();
    let result = verify_dsse_signature_legacy_for_tests(&content_str, &envelope, &trust_store);

    assert!(
        matches!(result, Err(RegistryError::SignatureInvalid { .. })),
        "Should reject invalid signature: {:?}",
        result
    );
}
