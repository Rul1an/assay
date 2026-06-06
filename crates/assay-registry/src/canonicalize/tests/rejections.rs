use super::support::*;

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
