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
    let raw = crate::verify::compute_digest_raw(content);

    // They should be different!
    assert_ne!(canonical, raw);

    // Raw is what we had before (review golden vector)
    assert_eq!(
        raw,
        "sha256:5a9a6b1e95e8c1d36779b87212835c9bfa9cae5d98cb9c75fb8c478750e5e200"
    );
}

#[test]
#[allow(deprecated)]
fn test_compute_digest_raw_matches_bytes_helper() {
    // Use clearly non-canonical, malformed YAML-like text to avoid ambiguity:
    // this contract freezes raw byte hashing parity only.
    let content = "this is not: valid: yaml: [[";
    let wrapped = crate::verify::compute_digest_raw(content);
    let helper = crate::digest::sha256_hex_bytes(content.as_bytes());
    assert_eq!(wrapped, helper);
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

// ==================== DSSE Test Vectors (SPEC §6.3) ====================

/// Helper to create a signing key from a deterministic seed.
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
    use pkcs8::EncodePublicKey;
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
    use pkcs8::EncodePublicKey;
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
    use pkcs8::EncodePublicKey;
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

#[test]
fn test_verify_pack_fail_closed_matrix_contract() {
    let trust_store = TrustStore::new();
    let content = "name: test-pack\nversion: \"1.0.0\"\nrules: []";
    let digest = compute_digest(content);

    let unsigned = make_fetch_result(content, Some(digest.clone()), None, None);

    // Unsigned is rejected by default.
    let err_unsigned_default = verify_pack(&unsigned, &trust_store, &VerifyOptions::default())
        .expect_err("unsigned pack must fail closed by default");
    assert!(matches!(
        err_unsigned_default,
        RegistryError::Unsigned { .. }
    ));

    // Unsigned may pass only when explicitly allowed.
    let allowed = verify_pack(
        &unsigned,
        &trust_store,
        &VerifyOptions::default().allow_unsigned(),
    )
    .expect("allow_unsigned should permit unsigned input");
    assert!(!allowed.signed);
    assert!(allowed.key_id.is_none());
    assert_eq!(allowed.digest, digest);

    // Digest mismatch must still fail closed even when allow_unsigned is enabled.
    let mismatch = make_fetch_result(
        content,
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()),
        None,
        None,
    );
    let err_mismatch = verify_pack(
        &mismatch,
        &trust_store,
        &VerifyOptions::default().allow_unsigned(),
    )
    .expect_err("digest mismatch must fail closed before signature policy");
    assert!(matches!(err_mismatch, RegistryError::DigestMismatch { .. }));
}

#[test]
fn test_verify_pack_malformed_signature_reason_is_stable() {
    let trust_store = TrustStore::new();
    let content = "name: malformed-signature\nversion: \"1.0.0\"";
    let digest = compute_digest(content);
    let malformed = make_fetch_result(
        content,
        Some(digest),
        Some("not base64 envelope".to_string()),
        None,
    );

    let err = verify_pack(&malformed, &trust_store, &VerifyOptions::default())
        .expect_err("malformed signature header must fail closed");
    match err {
        RegistryError::SignatureInvalid { reason } => {
            assert!(
                reason.starts_with("invalid base64 envelope:"),
                "reason prefix drifted: {reason}"
            );
        }
        other => panic!("expected SignatureInvalid for malformed signature, got {other:?}"),
    }
}

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
    use pkcs8::EncodePublicKey;
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

    use pkcs8::EncodePublicKey;
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
    use pkcs8::EncodePublicKey;
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

    use pkcs8::EncodePublicKey;
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
