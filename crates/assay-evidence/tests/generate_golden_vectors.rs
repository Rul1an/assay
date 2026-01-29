//! Generate golden test vectors per fixture schema v1.
//!
//! Run: cargo test -p assay-evidence --test generate_golden_vectors -- --nocapture
//!
//! Outputs:
//! - tests/fixtures/keys/test_ed25519_private_pkcs8.pem
//! - tests/fixtures/keys/test_ed25519_public_spki.pem
//! - crates/assay-evidence/tests/fixtures/mandate_golden_vectors.json

use assay_evidence::mandate::MANDATE_PAYLOAD_TYPE;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{pkcs8::EncodePrivateKey, pkcs8::EncodePublicKey, Signer, SigningKey};
use serde_json::json;
use sha2::{Digest, Sha256};

/// Deterministic test key seed (DO NOT USE IN PRODUCTION).
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn test_key() -> SigningKey {
    SigningKey::from_bytes(&TEST_KEY_SEED)
}

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

fn generate_vector(
    name: &str,
    description: &str,
    hashable_content: serde_json::Value,
    key: &SigningKey,
    key_id: &str,
) -> serde_json::Value {
    // Phase 1: mandate_id
    let hashable_jcs = serde_jcs::to_string(&hashable_content).unwrap();
    let hashable_bytes = hashable_jcs.as_bytes();
    let mandate_id = format!("sha256:{}", hex::encode(Sha256::digest(hashable_bytes)));

    // Phase 2: signable content
    let mut signable_content = hashable_content.clone();
    if let Some(obj) = signable_content.as_object_mut() {
        obj.insert("mandate_id".to_string(), json!(mandate_id.clone()));
    }
    let signable_jcs = serde_jcs::to_string(&signable_content).unwrap();
    let signable_bytes = signable_jcs.as_bytes();

    // PAE
    let pae_bytes = build_pae(MANDATE_PAYLOAD_TYPE, signable_bytes);

    // Signature
    let signature = key.sign(&pae_bytes);
    let signature_b64 = BASE64.encode(signature.to_bytes());

    // payload_digest (content_id in v1.0.2)
    let payload_digest = mandate_id.clone();

    // signed_payload_digest
    let signed_payload_digest = format!("sha256:{}", hex::encode(Sha256::digest(signable_bytes)));

    // Build final event
    let mut final_data = signable_content.clone();
    if let Some(obj) = final_data.as_object_mut() {
        obj.insert(
            "signature".to_string(),
            json!({
                "version": 1,
                "algorithm": "ed25519",
                "payload_type": MANDATE_PAYLOAD_TYPE,
                "content_id": payload_digest,
                "signed_payload_digest": signed_payload_digest,
                "key_id": key_id,
                "signature": signature_b64
            }),
        );
    }

    json!({
        "name": name,
        "description": description,
        "hashable_content": hashable_content,
        "hashable_canonical_jcs": hashable_jcs,
        "expected_mandate_id": mandate_id,
        "signable_content": signable_content,
        "signable_canonical_jcs": signable_jcs,
        "expected_pae_b64": BASE64.encode(&pae_bytes),
        "signature": {
            "version": 1,
            "algorithm": "ed25519",
            "payload_type": MANDATE_PAYLOAD_TYPE,
            "content_id": payload_digest,
            "signed_payload_digest": signed_payload_digest,
            "key_id": key_id,
            "expected_signature_b64": signature_b64
        },
        "final_event": {
            "specversion": "1.0",
            "id": format!("evt_{}", name.replace("-", "_")),
            "type": "assay.mandate.v1",
            "source": "assay://test/golden-vectors",
            "time": "2026-01-28T10:00:00Z",
            "datacontenttype": "application/json",
            "data": final_data
        }
    })
}

#[test]
fn generate_golden_vectors_json() {
    let key = test_key();

    // Compute key_id from SPKI
    let spki_der = key.verifying_key().to_public_key_der().unwrap();
    let key_id = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(spki_der.as_bytes()))
    );

    // Export keys as DER (base64)
    let private_pkcs8_der = key.to_pkcs8_der().unwrap();
    let private_pkcs8_b64 = BASE64.encode(private_pkcs8_der.as_bytes());
    let public_spki_b64 = BASE64.encode(spki_der.as_bytes());

    println!("=== TEST KEY (DO NOT USE IN PRODUCTION) ===\n");
    println!("key_id: {}\n", key_id);
    println!("private_key_pkcs8_der_b64: {}\n", private_pkcs8_b64);
    println!("public_key_spki_der_b64: {}\n", public_spki_b64);

    // Vector 1: Intent mandate (basic)
    let intent_content = json!({
        "constraints": {},
        "context": {
            "audience": "myorg/app",
            "issuer": "auth.myorg.com"
        },
        "mandate_kind": "intent",
        "principal": {
            "method": "oidc",
            "subject": "user-123"
        },
        "scope": {
            "operation_class": "read",
            "tools": ["search_*"]
        },
        "validity": {
            "issued_at": "2026-01-28T10:00:00Z"
        }
    });

    let v1 = generate_vector(
        "intent-basic",
        "Minimal intent mandate for discovery operations",
        intent_content,
        &key,
        &key_id,
    );

    // Vector 2: Transaction mandate with transaction_ref
    let txn_content = json!({
        "constraints": {
            "max_uses": 1,
            "require_confirmation": true,
            "single_use": true
        },
        "context": {
            "audience": "acme-corp/shopping-agent",
            "issuer": "auth.acme-corp.com",
            "nonce": "confirm_session_xyz789",
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-b7ad6b7169203331-01"
        },
        "mandate_kind": "transaction",
        "principal": {
            "method": "oidc",
            "subject": "usr_test_001"
        },
        "scope": {
            "max_value": {
                "amount": "99.99",
                "currency": "USD"
            },
            "operation_class": "commit",
            "tools": ["purchase_item"],
            "transaction_ref": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "validity": {
            "expires_at": "2026-01-28T10:35:00Z",
            "issued_at": "2026-01-28T10:30:00Z",
            "not_before": "2026-01-28T10:30:00Z"
        }
    });

    let v2 = generate_vector(
        "txn-commit-with-transaction-ref",
        "Transaction mandate with commit scope + transaction_ref binding",
        txn_content,
        &key,
        &key_id,
    );

    // Vector 3: Escaped glob pattern
    let escaped_glob_content = json!({
        "constraints": {},
        "context": {
            "audience": "test/escaped-globs",
            "issuer": "auth.test.com"
        },
        "mandate_kind": "intent",
        "principal": {
            "method": "oidc",
            "subject": "user-glob-test"
        },
        "scope": {
            "operation_class": "read",
            "tools": ["fs.\\*\\*", "path\\\\to\\*"]
        },
        "validity": {
            "issued_at": "2026-01-28T10:00:00Z"
        }
    });

    let v3 = generate_vector(
        "intent-escaped-glob",
        "Intent mandate with escaped glob patterns (\\* and \\\\)",
        escaped_glob_content,
        &key,
        &key_id,
    );

    // Build complete fixture
    let fixture = json!({
        "meta": {
            "format_version": "1",
            "payload_type": MANDATE_PAYLOAD_TYPE,
            "hash": "sha256",
            "jcs": "rfc8785",
            "dsse_pae": "dssev1",
            "signature_algorithm": "ed25519",
            "base64": "rfc4648_padded"
        },
        "test_key": {
            "key_id": key_id,
            "public_key_spki_der_b64": public_spki_b64,
            "seed_hex": hex::encode(TEST_KEY_SEED),
            "_note": "Generate SigningKey from seed_hex: SigningKey::from_bytes(&hex::decode(seed_hex))",
            "_warning": "TEST KEY ONLY - DO NOT USE IN PRODUCTION"
        },
        "vectors": [v1, v2, v3]
    });

    println!("\n=== GOLDEN VECTORS JSON ===\n");
    println!("{}", serde_json::to_string_pretty(&fixture).unwrap());

    // Verify all vectors
    for (i, v) in [&v1, &v2, &v3].iter().enumerate() {
        let name = v["name"].as_str().unwrap();
        let expected_mandate_id = v["expected_mandate_id"].as_str().unwrap();
        let hashable_jcs = v["hashable_canonical_jcs"].as_str().unwrap();
        let computed_id = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(hashable_jcs.as_bytes()))
        );
        assert_eq!(
            computed_id, expected_mandate_id,
            "Vector {} ({}) mandate_id mismatch",
            i, name
        );
        println!("✅ Vector {} ({}) verified", i, name);
    }

    // Write to fixture file
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mandate_golden_vectors.json");
    std::fs::write(
        &fixture_path,
        serde_json::to_string_pretty(&fixture).unwrap(),
    )
    .expect("Failed to write fixture file");
    println!("\n✅ Written to: {}", fixture_path.display());
}
