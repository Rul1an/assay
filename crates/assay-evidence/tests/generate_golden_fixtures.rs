//! Generate golden test fixtures with real cryptographic signatures.
//!
//! Run with: cargo test -p assay-evidence --test generate_golden_fixtures -- --nocapture
//!
//! This generates:
//! - tests/fixtures/keys/test_ed25519.pem (PKCS#8 private key)
//! - tests/fixtures/keys/test_ed25519_pub.pem (SPKI public key)
//! - tests/fixtures/mandate/golden_signed_mandate.json (complete signed mandate)
//! - tests/fixtures/mandate/golden_vectors.json (all intermediate values)

use assay_evidence::crypto::jcs;
use assay_evidence::mandate::{
    compute_mandate_id, sign_mandate, AuthMethod, Constraints, Context, MandateContent,
    MandateKind, OperationClass, Principal, Scope, Validity, MANDATE_PAYLOAD_TYPE,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Deterministic test key seed (DO NOT USE IN PRODUCTION).
/// This is a well-known test value for reproducible fixtures.
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn test_key() -> SigningKey {
    SigningKey::from_bytes(&TEST_KEY_SEED)
}

fn golden_content() -> MandateContent {
    MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()]).with_operation_class(OperationClass::Read),
        validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    }
}

#[derive(Serialize)]
struct SignableMandate {
    constraints: assay_evidence::mandate::Constraints,
    context: assay_evidence::mandate::Context,
    mandate_id: String,
    mandate_kind: MandateKind,
    principal: assay_evidence::mandate::Principal,
    scope: assay_evidence::mandate::Scope,
    validity: assay_evidence::mandate::Validity,
}

#[test]
fn generate_golden_fixtures() {
    let key = test_key();
    let content = golden_content();

    // === Phase 1: mandate_id computation ===
    let hashable_jcs = jcs::to_string(&content).unwrap();
    let hashable_bytes = hashable_jcs.as_bytes();
    let mandate_id = compute_mandate_id(&content).unwrap();

    println!("=== GOLDEN VECTORS FOR SPEC-Mandate-v1.0.2 ===\n");

    println!("## Test Key (DO NOT USE IN PRODUCTION)");
    println!("seed_hex: {}", hex::encode(TEST_KEY_SEED));

    // Get key_id
    use ed25519_dalek::pkcs8::EncodePublicKey;
    let spki_der = key
        .verifying_key()
        .to_public_key_der()
        .expect("SPKI encode");
    let key_id = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(spki_der.as_bytes()))
    );
    println!("key_id: {}\n", key_id);

    println!("## Phase 1: mandate_id = sha256(JCS(hashable_content))");
    println!("hashable_jcs:");
    println!("{}\n", hashable_jcs);
    println!("hashable_bytes_hex:");
    println!("{}\n", hex::encode(hashable_bytes));
    println!("sha256_hex: {}", &mandate_id[7..]); // strip sha256:
    println!("mandate_id: {}\n", mandate_id);

    // === Phase 2: signing ===
    let signable = SignableMandate {
        constraints: content.constraints.clone(),
        context: content.context.clone(),
        mandate_id: mandate_id.clone(),
        mandate_kind: content.mandate_kind,
        principal: content.principal.clone(),
        scope: content.scope.clone(),
        validity: content.validity.clone(),
    };

    let signable_jcs = jcs::to_string(&signable).unwrap();
    let signable_bytes = signable_jcs.as_bytes();
    let signed_payload_digest = format!("sha256:{}", hex::encode(Sha256::digest(signable_bytes)));

    println!("## Phase 2: signature = ed25519_sign(key, PAE(type, JCS(signable)))");
    println!("signable_jcs:");
    println!("{}\n", signable_jcs);
    println!("signable_bytes_hex:");
    println!("{}\n", hex::encode(signable_bytes));
    println!("signed_payload_digest: {}\n", signed_payload_digest);

    // Build PAE
    let pae = build_pae(MANDATE_PAYLOAD_TYPE, signable_bytes);
    println!("payload_type: {}", MANDATE_PAYLOAD_TYPE);
    println!("payload_type_length: {}", MANDATE_PAYLOAD_TYPE.len());
    println!("signable_bytes_length: {}", signable_bytes.len());
    println!("\npae_string:");
    println!("{}\n", String::from_utf8_lossy(&pae));
    println!("pae_bytes_hex:");
    println!("{}\n", hex::encode(&pae));
    println!("pae_bytes_base64:");
    println!("{}\n", BASE64.encode(&pae));

    // Sign
    use ed25519_dalek::Signer;
    let signature = key.sign(&pae);
    let signature_b64 = BASE64.encode(signature.to_bytes());

    println!("signature_bytes_hex: {}", hex::encode(signature.to_bytes()));
    println!("signature_base64: {}\n", signature_b64);

    // === Full signed mandate ===
    let signed_mandate = sign_mandate(&content, &key).unwrap();
    let signed_json = serde_json::to_string_pretty(&signed_mandate).unwrap();

    println!("## Complete Signed Mandate (JSON)");
    println!("{}\n", signed_json);

    // === Output golden vectors JSON ===
    let golden = serde_json::json!({
        "_comment": "Golden test vectors for SPEC-Mandate-v1.0.2 cross-language interop",
        "_generated_by": "cargo test -p assay-evidence --test generate_golden_fixtures",
        "_warning": "Test key only - DO NOT USE IN PRODUCTION",

        "test_key": {
            "seed_hex": hex::encode(TEST_KEY_SEED),
            "key_id": key_id,
            "algorithm": "ed25519"
        },

        "phase1_mandate_id": {
            "hashable_jcs": hashable_jcs,
            "hashable_bytes_hex": hex::encode(hashable_bytes),
            "sha256_hex": &mandate_id[7..],
            "mandate_id": mandate_id.clone()
        },

        "phase2_signing": {
            "signable_jcs": signable_jcs,
            "signable_bytes_hex": hex::encode(signable_bytes),
            "signed_payload_digest": signed_payload_digest,
            "payload_type": MANDATE_PAYLOAD_TYPE,
            "payload_type_length": MANDATE_PAYLOAD_TYPE.len(),
            "signable_bytes_length": signable_bytes.len(),
            "pae_bytes_hex": hex::encode(&pae),
            "pae_bytes_base64": BASE64.encode(&pae),
            "signature_bytes_hex": hex::encode(signature.to_bytes()),
            "signature_base64": signature_b64
        },

        "complete_signed_mandate": signed_mandate
    });

    println!("## Golden Vectors JSON (for fixtures file)");
    println!("{}", serde_json::to_string_pretty(&golden).unwrap());

    // Verify the signature works
    use assay_evidence::mandate::verify_mandate;
    let result = verify_mandate(&signed_mandate, &key.verifying_key());
    assert!(result.is_ok(), "Golden mandate must verify: {:?}", result);
    println!("\n✅ Signature verified successfully");
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
