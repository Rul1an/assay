use super::support::*;

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
