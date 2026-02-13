//! Canonicalize behavior freeze tests.
//!
//! Kept in separate file so mod.rs grep-gates (forbidden-knowledge) target
//! implementation code only, not test code.

use super::*;
use serde_json::Value as JsonValue;

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

// ==================== Strict YAML Rejection Tests ====================

#[test]
fn test_reject_anchor() {
    let yaml = "anchor: &myanchor value\nref: *myanchor";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::AnchorFound { .. })
    ));
}

#[test]
fn test_reject_alias() {
    let yaml = "ref: *undefined";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::AliasFound { .. })
    ));
}

#[test]
fn test_reject_anchor_in_sequence() {
    // Copilot: anchors in sequence items (e.g. - &a 1) must be rejected
    let yaml = "items:\n  - &anchor 1\n  - two";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::AnchorFound { .. })
    ));
}

#[test]
fn test_reject_alias_in_sequence() {
    // Copilot: aliases in sequence items (e.g. - *a) must be rejected
    let yaml = "items:\n  - *ref";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::AliasFound { .. })
    ));
}

#[test]
fn test_quoted_key_with_escaped_quote() {
    // Copilot: "a\"b": 1 must be recognized; find('"') alone would misparse
    let yaml = r#""a\"b": 1"#;
    let result = parse_yaml_strict(yaml);
    assert!(
        result.is_ok(),
        "quoted key with escaped quote should parse: {:?}",
        result
    );
    assert_eq!(result.unwrap()["a\"b"], 1);
}

#[test]
fn test_reject_tag_timestamp() {
    let yaml = "date: !!timestamp 2026-01-29";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::TagFound { .. })
    ));
}

#[test]
fn test_reject_tag_binary() {
    let yaml = "data: !!binary R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::TagFound { .. })
    ));
}

#[test]
fn test_reject_custom_tag() {
    let yaml = "value: !<tag:custom> data";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::TagFound { .. })
    ));
}

#[test]
fn test_reject_multi_document() {
    let yaml = "doc1: value\n---\ndoc2: value";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::MultiDocumentFound)
    ));
}

#[test]
fn test_reject_multi_document_start() {
    let yaml = "---\ndoc: value";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::MultiDocumentFound)
    ));
}

#[test]
fn test_reject_merge_key() {
    // YAML merge keys (<<) can cause duplicate-key-by-construction; reject.
    let yaml = "base: {a: 1}\n<<: {b: 2}";
    let result = parse_yaml_strict(yaml);
    assert!(
        result.is_err(),
        "Merge keys (<<) should be rejected: {:?}",
        result
    );
}

#[test]
fn test_reject_float() {
    let yaml = "value: 3.14159";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::FloatNotAllowed { .. })
    ));
}

#[test]
fn test_reject_float_scientific() {
    let yaml = "value: 1.5e10";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::FloatNotAllowed { .. })
    ));
}

#[test]
fn test_reject_integer_too_large() {
    let yaml = "value: 9007199254740993";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::IntegerOutOfRange { .. })
    ));
}

#[test]
fn test_accept_max_safe_integer() {
    let yaml = "value: 9007199254740992";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_accept_max_safe_integer_minus_one() {
    let yaml = "value: 9007199254740991";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_accept_min_safe_integer() {
    let yaml = "value: -9007199254740992";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_accept_min_safe_integer_plus_one() {
    let yaml = "value: -9007199254740991";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_reject_integer_too_negative() {
    let yaml = "value: -9007199254740993";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::IntegerOutOfRange { .. })
    ));
}

// ==================== DoS Limits Tests ====================

#[test]
fn test_reject_deep_nesting() {
    let mut yaml = String::from("a:\n");
    for i in 0..60 {
        yaml.push_str(&"  ".repeat(i + 1));
        yaml.push_str("b:\n");
    }
    yaml.push_str(&"  ".repeat(61));
    yaml.push_str("c: value");
    assert!(matches!(
        parse_yaml_strict(&yaml),
        Err(CanonicalizeError::MaxDepthExceeded { .. })
    ));
}

#[test]
fn test_accept_reasonable_depth() {
    let mut yaml = String::from("a:\n");
    for i in 0..10 {
        yaml.push_str(&"  ".repeat(i + 1));
        yaml.push_str("b:\n");
    }
    yaml.push_str(&"  ".repeat(11));
    yaml.push_str("c: value");
    assert!(parse_yaml_strict(&yaml).is_ok());
}

#[test]
fn test_reject_input_too_large() {
    let yaml = "x".repeat(MAX_TOTAL_SIZE + 1);
    assert!(matches!(
        parse_yaml_strict(&yaml),
        Err(CanonicalizeError::InputTooLarge { .. })
    ));
}

// ==================== Edge Cases ====================

#[test]
fn test_ampersand_in_string_allowed() {
    let yaml = r#"text: "this & that""#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_asterisk_in_string_allowed() {
    let yaml = r#"pattern: "*.txt""#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_triple_dash_in_string_allowed() {
    let yaml = r#"divider: "---""#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_exclamation_in_string_allowed() {
    let yaml = r#"message: "Hello!! World!!""#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_empty_yaml() {
    let yaml = "";
    let result = parse_yaml_strict(yaml);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), JsonValue::Null);
}

#[test]
fn test_null_value() {
    assert!(parse_yaml_strict("value: null").is_ok());
}

#[test]
fn test_boolean_values() {
    let result = parse_yaml_strict("enabled: true\ndisabled: false");
    assert!(result.is_ok());
    let json = result.unwrap();
    assert_eq!(json["enabled"], true);
    assert_eq!(json["disabled"], false);
}

#[test]
fn test_integer_values() {
    let result = parse_yaml_strict("positive: 42\nnegative: -17\nzero: 0");
    assert!(result.is_ok());
    let json = result.unwrap();
    assert_eq!(json["positive"], 42);
    assert_eq!(json["negative"], -17);
    assert_eq!(json["zero"], 0);
}

#[test]
fn test_array_values() {
    let result = parse_yaml_strict("items:\n  - one\n  - two\n  - three");
    assert!(result.is_ok());
    let json = result.unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 3);
}

#[test]
fn test_nested_structure() {
    let result = parse_yaml_strict("outer:\n  inner:\n    value: test");
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["outer"]["inner"]["value"], "test");
}

// ==================== Digest Determinism ====================

#[test]
fn test_digest_deterministic() {
    let yaml = "name: test\nversion: \"1.0.0\"\nkind: pack";
    let d1 = compute_canonical_digest(yaml).unwrap();
    let d2 = compute_canonical_digest(yaml).unwrap();
    let d3 = compute_canonical_digest(yaml).unwrap();
    assert_eq!(d1, d2);
    assert_eq!(d2, d3);
}

#[test]
fn test_digest_format() {
    let digest = compute_canonical_digest("test: value").unwrap();
    assert!(digest.starts_with("sha256:"));
    assert_eq!(digest.len(), 7 + 64);
    assert!(digest[7..].chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_whitespace_normalization() {
    let d1 = compute_canonical_digest("a: 1\nb: 2").unwrap();
    let d2 = compute_canonical_digest("a:   1\nb:    2").unwrap();
    let d3 = compute_canonical_digest("a: 1\n\nb: 2").unwrap();
    assert_eq!(d1, d2);
    assert_eq!(d2, d3);
}

// ==================== Duplicate Key Detection ====================

#[test]
fn test_reject_duplicate_keys_top_level() {
    let yaml = "name: first\nversion: \"1.0.0\"\nname: second";
    let result = parse_yaml_strict(yaml);
    assert!(
        matches!(result, Err(CanonicalizeError::DuplicateKey { ref key }) if key == "name"),
        "{:?}",
        result
    );
}

#[test]
fn test_reject_duplicate_keys_nested() {
    let yaml = "outer:\n  inner: 1\n  inner: 2";
    let result = parse_yaml_strict(yaml);
    assert!(
        matches!(
            result,
            Err(CanonicalizeError::DuplicateKey { .. }) | Err(CanonicalizeError::ParseError { .. })
        ),
        "{:?}",
        result
    );
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(
            msg.contains("inner") || msg.contains("duplicate"),
            "{}",
            msg
        );
    }
}

#[test]
fn test_reject_duplicate_keys_different_values() {
    let yaml = "config: true\nconfig: some_string";
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::DuplicateKey { .. })
    ));
}

#[test]
fn test_allow_same_key_different_levels() {
    let yaml = "name: outer\nnested:\n  name: inner";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_allow_unique_keys() {
    assert!(parse_yaml_strict("name: test\nversion: \"1.0.0\"\nkind: pack").is_ok());
}

// ==================== Single Quote / Block Scalar ====================

#[test]
fn test_ampersand_in_single_quoted_string() {
    assert!(parse_yaml_strict("text: 'this & that'").is_ok());
}

#[test]
fn test_asterisk_in_single_quoted_string() {
    assert!(parse_yaml_strict("pattern: '*.txt'").is_ok());
}

#[test]
fn test_tag_in_quoted_string_allowed() {
    assert!(parse_yaml_strict(r#"message: "Use !!binary for base64""#).is_ok());
}

#[test]
fn test_quoted_key_with_special_chars() {
    assert!(parse_yaml_strict(r#""key:with:colons": value"#).is_ok());
}

#[test]
fn test_duplicate_quoted_keys() {
    let yaml = r#""name": first
"name": second"#;
    assert!(matches!(
        parse_yaml_strict(yaml),
        Err(CanonicalizeError::DuplicateKey { .. })
    ));
}

// ==================== Flow Mapping ====================

#[test]
fn test_flow_mapping_simple_allowed() {
    assert!(parse_yaml_strict("config: {a: 1, b: 2}").is_ok());
}

#[test]
fn test_flow_mapping_duplicate_detected_by_serde() {
    let result = parse_yaml_strict("config: {a: 1, a: 2}");
    assert!(matches!(result, Err(CanonicalizeError::ParseError { .. })));
}

#[test]
fn test_top_level_flow_mapping_duplicate() {
    let result = parse_yaml_strict("{a: 1, a: 2}");
    assert!(matches!(result, Err(CanonicalizeError::ParseError { .. })));
}

#[test]
fn test_complex_key_rejected() {
    let result = parse_yaml_strict("? [a, b]\n: value");
    assert!(result.is_err());
}

// ==================== List Item Scoping ====================

#[test]
fn test_list_items_same_key_allowed() {
    let yaml = "items:\n  - name: first\n  - name: second";
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_list_item_duplicate_within_same_item() {
    let yaml = "items:\n  - name: first\n    name: second";
    let result = parse_yaml_strict(yaml);
    assert!(matches!(
        result,
        Err(CanonicalizeError::DuplicateKey { .. }) | Err(CanonicalizeError::ParseError { .. })
    ));
}

#[test]
fn test_top_level_sequence_same_keys() {
    assert!(parse_yaml_strict("- a: 1\n- a: 2").is_ok());
}

#[test]
fn test_top_level_sequence_duplicate_in_item() {
    let yaml = "- a: 1\n  a: 2";
    let result = parse_yaml_strict(yaml);
    assert!(matches!(
        result,
        Err(CanonicalizeError::DuplicateKey { .. }) | Err(CanonicalizeError::ParseError { .. })
    ));
}

#[test]
fn test_nested_list_with_mappings() {
    let yaml = r#"rules:
  - id: rule1
    name: First Rule
  - id: rule2
    name: Second Rule"#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_deeply_nested_list_same_keys() {
    let yaml = r#"outer:
  inner:
    - key: val1
      other: a
    - key: val2
      other: b"#;
    assert!(parse_yaml_strict(yaml).is_ok());
}

#[test]
fn test_mixed_sequence_and_mapping() {
    let yaml = r#"items:
  - name: item1
  - name: item2
metadata:
  name: should_not_conflict"#;
    assert!(parse_yaml_strict(yaml).is_ok());
}
