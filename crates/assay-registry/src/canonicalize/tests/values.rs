use super::support::*;

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
