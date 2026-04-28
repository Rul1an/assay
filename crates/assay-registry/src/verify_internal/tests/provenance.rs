use super::*;

// ==================== P0 Fix: Canonical Bytes Verification Tests ====================

#[test]
fn test_verify_pack_uses_canonical_bytes() {
    // This test verifies the P0 fix: verify_pack canonicalizes content
    // before DSSE verification (not comparing raw YAML to canonical payload)
    let seed: [u8; 32] = [0x88; 32];
    let signing_key = keypair_from_seed(seed);

    // Content with keys in non-canonical order
    let content = "z: 3\na: 1\nm: 2";

    // Create envelope (uses canonical form internally)
    let (envelope, key_id) = create_signed_envelope(&signing_key, content);

    // Add key to trust store
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

    // Create FetchResult with RAW YAML content (not canonical)
    let fetch_result = FetchResult {
        content: content.to_string(), // Raw YAML, keys NOT sorted
        headers: crate::types::PackHeaders {
            digest: Some(compute_digest(content)),
            signature: Some(BASE64.encode(serde_json::to_vec(&envelope).unwrap())),
            key_id: envelope.signatures.first().map(|s| s.key_id.clone()),
            etag: None,
            cache_control: None,
            content_length: None,
        },
        computed_digest: compute_digest(content),
    };

    // verify_pack should work because it canonicalizes before comparison
    let result = verify_pack(&fetch_result, &trust_store, &VerifyOptions::default());
    assert!(
        result.is_ok(),
        "verify_pack should canonicalize content before DSSE verification: {:?}",
        result
    );
}

#[test]
fn test_verify_pack_canonicalization_equivalent_yaml_variants_contract() {
    // Equivalent YAML representations must verify identically after canonicalization.
    let seed: [u8; 32] = [0x90; 32];
    let signing_key = keypair_from_seed(seed);

    let source_yaml = "z: 3\na: 1\nm: 2";
    let variant_yaml = "a: 1\nm: 2\nz: 3\n";
    assert_eq!(compute_digest(source_yaml), compute_digest(variant_yaml));

    let source_canonical = canonicalize_for_dsse(source_yaml).unwrap();
    let variant_canonical = canonicalize_for_dsse(variant_yaml).unwrap();
    assert_eq!(source_canonical, variant_canonical);

    // Non-equivalent content must not collapse to the same digest/canonical bytes.
    let changed_yaml = "a: 1\nm: 2\nz: 4\n";
    assert_ne!(compute_digest(source_yaml), compute_digest(changed_yaml));
    assert_ne!(
        source_canonical,
        canonicalize_for_dsse(changed_yaml).unwrap()
    );

    let (envelope, key_id) = create_signed_envelope(&signing_key, source_yaml);

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

    let fetch_result = make_fetch_result(
        variant_yaml,
        Some(compute_digest(variant_yaml)),
        Some(BASE64.encode(serde_json::to_vec(&envelope).unwrap())),
        envelope.signatures.first().map(|s| s.key_id.clone()),
    );

    let result = verify_pack(&fetch_result, &trust_store, &VerifyOptions::default());
    assert!(
        result.is_ok(),
        "canonical-equivalent YAML variant should verify: {:?}",
        result
    );
}

#[test]
fn test_canonical_bytes_differ_from_raw() {
    // Demonstrate why the fix matters: raw != canonical
    let yaml = "z: 1\na: 2\nm: 3"; // Keys not sorted

    // Raw YAML bytes
    let raw_bytes = yaml.as_bytes();

    // Canonical JCS bytes (keys sorted: a, m, z)
    let canonical_bytes = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(yaml).unwrap(),
    )
    .unwrap();

    // They MUST be different!
    assert_ne!(
        raw_bytes,
        &canonical_bytes[..],
        "Raw YAML and canonical JCS MUST differ for non-sorted keys"
    );

    // Canonical form should have sorted keys
    let canonical_str = String::from_utf8(canonical_bytes).unwrap();
    assert!(
        canonical_str.starts_with(r#"{"a":"#),
        "JCS must sort keys alphabetically, got: {}",
        canonical_str
    );
}

#[test]
fn test_verify_dsse_signature_bytes_directly() {
    // Test verify_dsse_signature_bytes function directly
    let seed: [u8; 32] = [0x99; 32];
    let signing_key = keypair_from_seed(seed);
    let content = "name: test\nversion: \"1.0.0\"";

    let (envelope, key_id) = create_signed_envelope(&signing_key, content);

    // Add key to trust store
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

    // Get canonical bytes
    let canonical_bytes = crate::canonicalize::to_canonical_jcs_bytes(
        &crate::canonicalize::parse_yaml_strict(content).unwrap(),
    )
    .unwrap();

    // Verify with canonical bytes should succeed
    let result = verify_dsse_signature_bytes(&canonical_bytes, &envelope, &trust_store);
    assert!(
        result.is_ok(),
        "Canonical bytes verification should succeed: {:?}",
        result
    );

    // Verify with raw YAML bytes should FAIL
    let raw_bytes = content.as_bytes();
    let result = verify_dsse_signature_bytes(raw_bytes, &envelope, &trust_store);
    assert!(
        matches!(result, Err(RegistryError::DigestMismatch { .. })),
        "Raw bytes should not match canonical payload: {:?}",
        result
    );
}

#[test]
fn test_verify_dsse_signature_legacy_helper_matches_bytes_path() {
    let seed: [u8; 32] = [0x31; 32];
    let signing_key = keypair_from_seed(seed);
    let content = "z: 3\na: 1\nm: 2";

    let (envelope, key_id) = create_signed_envelope(&signing_key, content);

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

    let legacy_ok = verify_dsse_signature_legacy_for_tests(content, &envelope, &trust_store);
    assert!(
        legacy_ok.is_ok(),
        "legacy helper should canonicalize input before verification: {:?}",
        legacy_ok
    );

    let canonical_bytes = canonicalize_for_dsse(content).unwrap();
    let bytes_ok = verify_dsse_signature_bytes(&canonical_bytes, &envelope, &trust_store);
    assert!(
        bytes_ok.is_ok(),
        "bytes API should verify the same canonical payload: {:?}",
        bytes_ok
    );

    let tampered_content = "z: 4\na: 1\nm: 2";
    let legacy_err =
        verify_dsse_signature_legacy_for_tests(tampered_content, &envelope, &trust_store);
    assert!(
        matches!(legacy_err, Err(RegistryError::DigestMismatch { .. })),
        "legacy helper should keep mismatch classification: {:?}",
        legacy_err
    );

    let tampered_canonical = canonicalize_for_dsse(tampered_content).unwrap();
    let bytes_err = verify_dsse_signature_bytes(&tampered_canonical, &envelope, &trust_store);
    assert!(
        matches!(bytes_err, Err(RegistryError::DigestMismatch { .. })),
        "bytes API should keep mismatch classification: {:?}",
        bytes_err
    );
}
