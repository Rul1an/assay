//! Payload type confusion prevention tests.
//!
//! Verifies that signature payload_type is strictly enforced to prevent
//! cross-type signature reuse attacks.

use assay_evidence::mandate::{
    sign_mandate, verify_mandate, AuthMethod, Constraints, Context, MandateContent, MandateKind,
    Principal, Scope, Validity, MANDATE_PAYLOAD_TYPE, MANDATE_REVOKED_PAYLOAD_TYPE,
    MANDATE_USED_PAYLOAD_TYPE,
};
use ed25519_dalek::SigningKey;

/// Deterministic test key for reproducibility.
fn test_key() -> SigningKey {
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    SigningKey::from_bytes(&seed)
}

fn create_test_content() -> MandateContent {
    MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()]),
        validity: Validity::now(),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    }
}

/// Test: Payload type constants are distinct.
#[test]
fn test_payload_types_are_distinct() {
    assert_ne!(
        MANDATE_PAYLOAD_TYPE, MANDATE_USED_PAYLOAD_TYPE,
        "Mandate and Used payload types must be distinct"
    );
    assert_ne!(
        MANDATE_PAYLOAD_TYPE, MANDATE_REVOKED_PAYLOAD_TYPE,
        "Mandate and Revoked payload types must be distinct"
    );
    assert_ne!(
        MANDATE_USED_PAYLOAD_TYPE, MANDATE_REVOKED_PAYLOAD_TYPE,
        "Used and Revoked payload types must be distinct"
    );

    println!("✅ Payload types are distinct:");
    println!("   MANDATE:  {}", MANDATE_PAYLOAD_TYPE);
    println!("   USED:     {}", MANDATE_USED_PAYLOAD_TYPE);
    println!("   REVOKED:  {}", MANDATE_REVOKED_PAYLOAD_TYPE);
}

/// Test: Payload type format follows media type convention.
#[test]
fn test_payload_type_format() {
    // All must be valid media types with vendor prefix and version
    for (name, pt) in [
        ("MANDATE", MANDATE_PAYLOAD_TYPE),
        ("USED", MANDATE_USED_PAYLOAD_TYPE),
        ("REVOKED", MANDATE_REVOKED_PAYLOAD_TYPE),
    ] {
        assert!(
            pt.starts_with("application/vnd.assay."),
            "{} payload type must use vendor prefix: {}",
            name,
            pt
        );
        assert!(
            pt.contains(";v="),
            "{} payload type must include version: {}",
            name,
            pt
        );
        assert!(
            pt.ends_with(";v=1"),
            "{} payload type must be v=1: {}",
            name,
            pt
        );
    }

    println!("✅ All payload types follow media type convention");
}

/// Test: Mandate signature has correct payload_type.
#[test]
fn test_mandate_signature_has_correct_payload_type() {
    let key = test_key();
    let content = create_test_content();

    let signed = sign_mandate(&content, &key).expect("signing should succeed");

    let sig = signed.signature.as_ref().expect("should have signature");
    assert_eq!(
        sig.payload_type, MANDATE_PAYLOAD_TYPE,
        "Mandate signature must use MANDATE_PAYLOAD_TYPE"
    );

    println!(
        "✅ Mandate signature uses correct payload_type: {}",
        sig.payload_type
    );
}

/// Test: Verification fails if signature payload_type is wrong.
#[test]
fn test_verification_fails_on_wrong_payload_type() {
    let key = test_key();
    let content = create_test_content();

    let mut signed = sign_mandate(&content, &key).expect("signing should succeed");

    // Tamper: change payload_type to used event type
    if let Some(ref mut sig) = signed.signature {
        sig.payload_type = MANDATE_USED_PAYLOAD_TYPE.to_string();
    }

    let result = verify_mandate(&signed, &key.verifying_key());

    assert!(
        result.is_err(),
        "Verification MUST fail with wrong payload_type"
    );

    let err = result.unwrap_err();
    assert!(
        format!("{:?}", err).contains("PayloadType"),
        "Error should mention payload type mismatch: {:?}",
        err
    );

    println!("✅ Type confusion attack correctly prevented");
    println!("   Error: {:?}", err);
}

/// Test: Verification fails if signature payload_type is revoked event type.
#[test]
fn test_verification_fails_on_revoked_payload_type() {
    let key = test_key();
    let content = create_test_content();

    let mut signed = sign_mandate(&content, &key).expect("signing should succeed");

    // Tamper: change payload_type to revoked event type
    if let Some(ref mut sig) = signed.signature {
        sig.payload_type = MANDATE_REVOKED_PAYLOAD_TYPE.to_string();
    }

    let result = verify_mandate(&signed, &key.verifying_key());

    assert!(
        result.is_err(),
        "Verification MUST fail with revoked payload_type on mandate"
    );

    println!("✅ Revoked type confusion attack correctly prevented");
}

/// Test: Verification fails if signature payload_type is arbitrary string.
#[test]
fn test_verification_fails_on_arbitrary_payload_type() {
    let key = test_key();
    let content = create_test_content();

    let mut signed = sign_mandate(&content, &key).expect("signing should succeed");

    // Tamper: change payload_type to arbitrary value
    if let Some(ref mut sig) = signed.signature {
        sig.payload_type = "application/json".to_string();
    }

    let result = verify_mandate(&signed, &key.verifying_key());

    assert!(
        result.is_err(),
        "Verification MUST fail with arbitrary payload_type"
    );

    println!("✅ Arbitrary payload_type correctly rejected");
}
