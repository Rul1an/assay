use super::support::*;

// ==================== Golden Vector Tests ====================

#[test]
fn test_golden_vector_basic_pack() {
    let yaml = "name: eu-ai-act-baseline\nversion: \"1.0.0\"\nkind: compliance";
    let digest = compute_canonical_digest(yaml).unwrap();
    assert_eq!(
        digest,
        "sha256:f47d932cdad4bde369ed0a7cf26fdcf4077777296346c4102d9017edbc62a070"
    );
}

#[test]
fn test_jcs_key_ordering() {
    let yaml1 = "z: 1\na: 2\nm: 3";
    let yaml2 = "a: 2\nm: 3\nz: 1";
    let yaml3 = "m: 3\nz: 1\na: 2";
    let digest1 = compute_canonical_digest(yaml1).unwrap();
    let digest2 = compute_canonical_digest(yaml2).unwrap();
    let digest3 = compute_canonical_digest(yaml3).unwrap();
    assert_eq!(digest1, digest2);
    assert_eq!(digest2, digest3);
}

#[test]
fn test_jcs_bytes_output() {
    let yaml = "name: test\nversion: \"1.0.0\"";
    let json = parse_yaml_strict(yaml).unwrap();
    let bytes = to_canonical_jcs_bytes(&json).unwrap();
    let expected = r#"{"name":"test","version":"1.0.0"}"#;
    assert_eq!(String::from_utf8(bytes).unwrap(), expected);
}

#[test]
fn test_digest_over_jcs_bytes_not_string() {
    // Regression guard: digest must be over JCS bytes (UTF-8), not over stringified JSON.
    // Non-ASCII ensures we're hashing bytes, not a string with encoding drift.
    let yaml = "name: café\nversion: \"1.0.0\"";
    let json = parse_yaml_strict(yaml).unwrap();
    let bytes = to_canonical_jcs_bytes(&json).unwrap();
    assert!(
        std::str::from_utf8(&bytes).is_ok(),
        "JCS output must be valid UTF-8"
    );
    let digest = digest::sha256_prefixed(&bytes);
    assert!(digest.starts_with("sha256:"));
    assert_eq!(digest.len(), 7 + 64);
    // Full flow: digest should match compute_canonical_digest
    assert_eq!(digest, compute_canonical_digest(yaml).unwrap());
}
