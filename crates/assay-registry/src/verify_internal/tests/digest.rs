use super::*;

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
