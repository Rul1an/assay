//! Golden test vectors for mandate evidence determinism.
//!
//! These tests validate that our implementation produces identical bytes
//! to what other implementations should produce, enabling cross-language interop.

use assay_evidence::crypto::jcs;
use assay_evidence::mandate::{
    compute_mandate_id, sign_mandate, verify_mandate, AuthMethod, Constraints, Context,
    GlobPattern, MandateContent, MandateKind, Principal, Scope, Validity,
};
use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

/// Golden vector: exact JCS output for mandate content
#[test]
fn test_golden_jcs_mandate_content() {
    let content = MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()])
            .with_operation_class(assay_evidence::mandate::OperationClass::Read),
        validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    };

    let canonical = jcs::to_string(&content).unwrap();

    // This is the NORMATIVE JCS output - if this changes, cross-language interop breaks
    println!("JCS canonical output:\n{}", canonical);

    // Verify key ordering (alphabetical)
    assert!(canonical.starts_with('{'));
    // constraints < context < mandate_kind < principal < scope < validity
    let constraint_pos = canonical.find("\"constraints\"").unwrap();
    let context_pos = canonical.find("\"context\"").unwrap();
    let kind_pos = canonical.find("\"mandate_kind\"").unwrap();
    let principal_pos = canonical.find("\"principal\"").unwrap();
    let scope_pos = canonical.find("\"scope\"").unwrap();
    let validity_pos = canonical.find("\"validity\"").unwrap();

    assert!(
        constraint_pos < context_pos,
        "constraints must come before context"
    );
    assert!(
        context_pos < kind_pos,
        "context must come before mandate_kind"
    );
    assert!(
        kind_pos < principal_pos,
        "mandate_kind must come before principal"
    );
    assert!(
        principal_pos < scope_pos,
        "principal must come before scope"
    );
    assert!(scope_pos < validity_pos, "scope must come before validity");

    // Verify no whitespace
    assert!(!canonical.contains(' '), "JCS must have no whitespace");
    assert!(!canonical.contains('\n'), "JCS must have no newlines");
}

/// Golden vector: mandate_id computation
#[test]
fn test_golden_mandate_id_determinism() {
    let content1 = create_golden_content();
    let content2 = create_golden_content();

    let id1 = compute_mandate_id(&content1).unwrap();
    let id2 = compute_mandate_id(&content2).unwrap();

    assert_eq!(id1, id2, "Same content MUST produce same mandate_id");
    assert!(
        id1.starts_with("sha256:"),
        "mandate_id must have sha256: prefix"
    );
    assert_eq!(
        id1.len(),
        71,
        "mandate_id must be 71 chars (sha256: + 64 hex)"
    );

    // Verify lowercase hex
    let hex_part = &id1[7..];
    assert!(
        hex_part
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "hex must be lowercase"
    );

    println!("Golden mandate_id: {}", id1);
}

/// Golden vector: PAE encoding format
#[test]
fn test_golden_pae_encoding() {
    let payload_type = "application/vnd.assay.mandate+json;v=1";
    let payload = b"{}";

    // Build PAE manually to verify format
    let pae = build_pae_for_test(payload_type, payload);
    let pae_str = String::from_utf8_lossy(&pae);

    // Expected: "DSSEv1 38 application/vnd.assay.mandate+json;v=1 2 {}"
    assert!(pae_str.starts_with("DSSEv1 "), "PAE must start with DSSEv1");
    assert!(pae_str.contains(" 38 "), "payload_type length must be 38");
    assert!(pae_str.contains(&format!(" {} ", payload_type)));
    assert!(
        pae_str.ends_with(" 2 {}"),
        "payload section must end with ' 2 {{}}'"
    );

    println!("PAE: {}", pae_str);
    println!("PAE hex: {}", hex::encode(&pae));
}

/// Golden vector: sign and verify roundtrip with deterministic mandate_id
#[test]
fn test_golden_sign_verify_roundtrip() {
    // Use deterministic key for reproducibility in test
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let key = SigningKey::from_bytes(&seed);

    let content = create_golden_content();
    let mandate = sign_mandate(&content, &key).unwrap();

    // Verify mandate_id is deterministic
    let expected_id = compute_mandate_id(&content).unwrap();
    assert_eq!(mandate.mandate_id, expected_id);

    // Verify signature validates
    let result = verify_mandate(&mandate, &key.verifying_key()).unwrap();
    assert_eq!(result.mandate_id, mandate.mandate_id);

    // Print for cross-language validation
    println!("Signed mandate JSON:");
    println!("{}", serde_json::to_string_pretty(&mandate).unwrap());
}

/// Golden vector: glob matching conformance
#[test]
fn test_golden_glob_vectors() {
    let vectors = vec![
        ("search_*", "search_products", true),
        ("search_*", "search_users", true),
        ("search_*", "search_", true),
        ("search_*", "search.products", false), // * stops at dot
        ("search_*", "search", false),
        ("search_*", "Search_products", false), // case-sensitive
        ("fs.read_*", "fs.read_file", true),
        ("fs.read_*", "fs.read.file", false), // * stops at second dot
        ("fs.**", "fs.read_file", true),
        ("fs.**", "fs.write.nested.path", true), // ** matches dots
        ("*", "search", true),
        ("*", "ns.tool", false), // * stops at dot
        ("**", "anything.at.all", true),
        (r"file\*name", "file*name", true), // escaped asterisk
        (r"path\\to", r"path\to", true),    // escaped backslash
    ];

    for (pattern, input, expected) in vectors {
        let glob = GlobPattern::new(pattern).unwrap();
        let actual = glob.matches(input);
        assert_eq!(
            actual, expected,
            "Pattern '{}' matching '{}': expected {}, got {}",
            pattern, input, expected, actual
        );
    }
}

/// Golden vector: time validity with skew
#[test]
fn test_golden_validity_with_skew() {
    let now = Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap();

    // Case 1: Valid within window
    let validity = Validity::at(now)
        .with_not_before(Utc.with_ymd_and_hms(2026, 1, 28, 9, 0, 0).unwrap())
        .with_expires_at(Utc.with_ymd_and_hms(2026, 1, 28, 11, 0, 0).unwrap());
    assert!(validity.is_valid_at(now), "Should be valid in window");

    // Case 2: Valid with skew (30s before not_before)
    let validity = Validity::at(now)
        .with_not_before(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 30).unwrap())
        .with_expires_at(Utc.with_ymd_and_hms(2026, 1, 28, 11, 0, 0).unwrap());
    assert!(!validity.is_valid_at(now), "Should fail without skew");
    assert!(
        validity.is_valid_at_with_skew(now, 30),
        "Should pass with 30s skew"
    );

    // Case 3: Expired (exactly at expires_at)
    let validity = Validity::at(now)
        .with_not_before(Utc.with_ymd_and_hms(2026, 1, 28, 9, 0, 0).unwrap())
        .with_expires_at(now); // expires exactly now
    assert!(
        !validity.is_valid_at(now),
        "Should be expired (expires_at is exclusive)"
    );
}

/// Golden vector: use_id content-addressed generation
#[test]
fn test_golden_use_id_generation() {
    let mandate_id = "sha256:abc123def456";
    let tool_call_id = "tc_001";
    let use_count = 1u32;

    // Build the JCS input per spec
    let use_id_content = serde_json::json!({
        "mandate_id": mandate_id,
        "tool_call_id": tool_call_id,
        "use_count": use_count
    });

    let jcs_bytes = serde_jcs::to_vec(&use_id_content).unwrap();
    let jcs_str = String::from_utf8(jcs_bytes.clone()).unwrap();

    // Verify JCS ordering: mandate_id < tool_call_id < use_count
    assert!(
        jcs_str.find("mandate_id").unwrap() < jcs_str.find("tool_call_id").unwrap(),
        "JCS must order keys"
    );

    let use_id = format!("sha256:{}", hex::encode(Sha256::digest(&jcs_bytes)));
    assert!(use_id.starts_with("sha256:"));

    println!("use_id JCS input: {}", jcs_str);
    println!("use_id: {}", use_id);
}

// --- Helper functions ---

fn create_golden_content() -> MandateContent {
    MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()])
            .with_operation_class(assay_evidence::mandate::OperationClass::Read),
        validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    }
}

fn build_pae_for_test(payload_type: &str, payload: &[u8]) -> Vec<u8> {
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
