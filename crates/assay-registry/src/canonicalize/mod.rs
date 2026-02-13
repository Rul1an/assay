//! YAML canonicalization for deterministic pack digests.
//!
//! Implements SPEC-Pack-Registry-v1 §6.1 (strict YAML subset) and §6.2 (canonical digest).
//!
//! # Strict YAML Subset
//!
//! The following YAML features are **rejected**:
//! - Anchors and aliases (`&name`, `*name`)
//! - Tags (`!!timestamp`, `!<custom>`)
//! - Multi-document (`---`)
//! - Duplicate keys (detected via pre-scan for block mappings, serde_yaml for flow)
//! - Floats (only integers allowed)
//! - Integers outside safe range (> 2^53)
//! - Non-string keys (complex keys like `? [a, b]`)
//!
//! # Supported Mapping Styles
//!
//! **Recommended**: Block mappings (one key per line)
//! ```yaml
//! name: my-pack
//! version: "1.0.0"
//! config:
//!   nested: value
//! ```
//!
//! **Allowed but not recommended**: Flow mappings
//! ```yaml
//! config: {a: 1, b: 2}
//! ```
//!
//! Flow mapping duplicate keys are detected by `serde_yaml` during parsing,
//! not by the pre-scan. Both detection methods result in rejection.
//!
//! # DoS Limits (§12.4)
//!
//! - Max depth: 50
//! - Max keys per mapping: 10,000
//! - Max string length: 1MB
//! - Max total size: 10MB

mod digest;
mod errors;
mod json;
mod yaml;

pub use errors::{
    CanonicalizeError, CanonicalizeResult, MAX_DEPTH, MAX_KEYS_PER_MAPPING, MAX_SAFE_INTEGER,
    MAX_STRING_LENGTH, MAX_TOTAL_SIZE, MIN_SAFE_INTEGER,
};
pub use json::to_canonical_jcs_bytes;
pub use yaml::parse_yaml_strict;

use crate::error::RegistryResult;

/// Compute canonical digest of YAML content.
///
/// Process:
/// 1. Parse YAML with strict validation
/// 2. Convert to JSON
/// 3. Serialize to JCS (RFC 8785)
/// 4. SHA-256 hash
/// 5. Format as `sha256:{hex}`
pub fn compute_canonical_digest(content: &str) -> Result<String, CanonicalizeError> {
    let json_value = parse_yaml_strict(content)?;
    let jcs_bytes = to_canonical_jcs_bytes(&json_value)?;
    Ok(digest::sha256_prefixed(&jcs_bytes))
}

/// Compute canonical digest, returning RegistryResult for API compatibility.
pub fn compute_canonical_digest_result(content: &str) -> RegistryResult<String> {
    compute_canonical_digest(content).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as JsonValue;

    // ==================== Golden Vector Tests ====================

    #[test]
    fn test_golden_vector_basic_pack() {
        // This is the golden vector from the review
        let yaml = "name: eu-ai-act-baseline\nversion: \"1.0.0\"\nkind: compliance";

        let digest = compute_canonical_digest(yaml).unwrap();

        // Expected JCS: {"kind":"compliance","name":"eu-ai-act-baseline","version":"1.0.0"}
        // Note: JCS sorts keys alphabetically
        assert_eq!(
            digest,
            "sha256:f47d932cdad4bde369ed0a7cf26fdcf4077777296346c4102d9017edbc62a070"
        );
    }

    #[test]
    fn test_jcs_key_ordering() {
        // Verify that key ordering is deterministic regardless of input order
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

        // JCS output should be deterministic with sorted keys
        let expected = r#"{"name":"test","version":"1.0.0"}"#;
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    // ==================== Strict YAML Rejection Tests ====================

    #[test]
    fn test_reject_anchor() {
        let yaml = "anchor: &myanchor value\nref: *myanchor";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::AnchorFound { .. })));
    }

    #[test]
    fn test_reject_alias() {
        // Even without anchor definition, alias syntax should fail
        let yaml = "ref: *undefined";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::AliasFound { .. })));
    }

    #[test]
    fn test_reject_tag_timestamp() {
        let yaml = "date: !!timestamp 2026-01-29";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::TagFound { .. })));
    }

    #[test]
    fn test_reject_tag_binary() {
        let yaml = "data: !!binary R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::TagFound { .. })));
    }

    #[test]
    fn test_reject_custom_tag() {
        let yaml = "value: !<tag:custom> data";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::TagFound { .. })));
    }

    #[test]
    fn test_reject_multi_document() {
        let yaml = "doc1: value\n---\ndoc2: value";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::MultiDocumentFound)));
    }

    #[test]
    fn test_reject_multi_document_start() {
        // Document separator at start
        let yaml = "---\ndoc: value";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(result, Err(CanonicalizeError::MultiDocumentFound)));
    }

    #[test]
    fn test_reject_float() {
        let yaml = "value: 3.14159";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(
            result,
            Err(CanonicalizeError::FloatNotAllowed { .. })
        ));
    }

    #[test]
    fn test_reject_float_scientific() {
        let yaml = "value: 1.5e10";
        let result = parse_yaml_strict(yaml);
        assert!(matches!(
            result,
            Err(CanonicalizeError::FloatNotAllowed { .. })
        ));
    }

    #[test]
    fn test_reject_integer_too_large() {
        let yaml = "value: 9007199254740993"; // 2^53 + 1
        let result = parse_yaml_strict(yaml);
        assert!(matches!(
            result,
            Err(CanonicalizeError::IntegerOutOfRange { .. })
        ));
    }

    #[test]
    fn test_accept_max_safe_integer() {
        let yaml = "value: 9007199254740992"; // 2^53 exactly
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accept_max_safe_integer_minus_one() {
        let yaml = "value: 9007199254740991"; // 2^53 - 1
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accept_min_safe_integer() {
        let yaml = "value: -9007199254740992"; // -2^53 exactly (boundary)
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accept_min_safe_integer_plus_one() {
        let yaml = "value: -9007199254740991"; // -2^53 + 1
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_integer_too_negative() {
        let yaml = "value: -9007199254740993"; // -2^53 - 1
        let result = parse_yaml_strict(yaml);
        assert!(matches!(
            result,
            Err(CanonicalizeError::IntegerOutOfRange { .. })
        ));
    }

    // ==================== DoS Limits Tests ====================

    #[test]
    fn test_reject_deep_nesting() {
        // Create deeply nested structure
        let mut yaml = String::from("a:\n");
        for i in 0..60 {
            yaml.push_str(&"  ".repeat(i + 1));
            yaml.push_str("b:\n");
        }
        yaml.push_str(&"  ".repeat(61));
        yaml.push_str("c: value");

        let result = parse_yaml_strict(&yaml);
        assert!(matches!(
            result,
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

        let result = parse_yaml_strict(&yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_input_too_large() {
        let yaml = "x".repeat(MAX_TOTAL_SIZE + 1);
        let result = parse_yaml_strict(&yaml);
        assert!(matches!(
            result,
            Err(CanonicalizeError::InputTooLarge { .. })
        ));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_ampersand_in_string_allowed() {
        // Ampersand inside a quoted string is fine
        let yaml = r#"text: "this & that""#;
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_asterisk_in_string_allowed() {
        // Asterisk inside a quoted string is fine
        let yaml = r#"pattern: "*.txt""#;
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_triple_dash_in_string_allowed() {
        // Triple dash inside a string is fine
        let yaml = r#"divider: "---""#;
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exclamation_in_string_allowed() {
        // Exclamation marks in strings are fine
        let yaml = r#"message: "Hello!! World!!""#;
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_yaml() {
        let yaml = "";
        let result = parse_yaml_strict(yaml);
        // Empty YAML parses to null
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), JsonValue::Null);
    }

    #[test]
    fn test_null_value() {
        let yaml = "value: null";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_boolean_values() {
        let yaml = "enabled: true\ndisabled: false";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["disabled"], false);
    }

    #[test]
    fn test_integer_values() {
        let yaml = "positive: 42\nnegative: -17\nzero: 0";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["positive"], 42);
        assert_eq!(json["negative"], -17);
        assert_eq!(json["zero"], 0);
    }

    #[test]
    fn test_array_values() {
        let yaml = "items:\n  - one\n  - two\n  - three";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        let items = json["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_nested_structure() {
        let yaml = "outer:\n  inner:\n    value: test";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["outer"]["inner"]["value"], "test");
    }

    // ==================== Digest Determinism ====================

    #[test]
    fn test_digest_deterministic() {
        let yaml = "name: test\nversion: \"1.0.0\"\nkind: pack";

        // Compute multiple times
        let digest1 = compute_canonical_digest(yaml).unwrap();
        let digest2 = compute_canonical_digest(yaml).unwrap();
        let digest3 = compute_canonical_digest(yaml).unwrap();

        assert_eq!(digest1, digest2);
        assert_eq!(digest2, digest3);
    }

    #[test]
    fn test_digest_format() {
        let yaml = "test: value";
        let digest = compute_canonical_digest(yaml).unwrap();

        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64); // "sha256:" + 64 hex chars
        assert!(digest[7..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_whitespace_normalization() {
        // Different whitespace should produce same digest after canonicalization
        let yaml1 = "a: 1\nb: 2";
        let yaml2 = "a:   1\nb:    2"; // Extra spaces
        let yaml3 = "a: 1\n\nb: 2"; // Extra newline

        let digest1 = compute_canonical_digest(yaml1).unwrap();
        let digest2 = compute_canonical_digest(yaml2).unwrap();
        let digest3 = compute_canonical_digest(yaml3).unwrap();

        // All should produce same canonical form
        assert_eq!(digest1, digest2);
        assert_eq!(digest2, digest3);
    }

    // ==================== Duplicate Key Detection Tests (P0 Fix) ====================

    #[test]
    fn test_reject_duplicate_keys_top_level() {
        // serde_yaml would merge these with "last wins", but we catch it in pre-scan
        let yaml = "name: first\nversion: \"1.0.0\"\nname: second";
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(result, Err(CanonicalizeError::DuplicateKey { ref key }) if key == "name"),
            "Should reject duplicate top-level key 'name': {:?}",
            result
        );
    }

    #[test]
    fn test_reject_duplicate_keys_nested() {
        // Duplicate keys at nested level
        // Note: serde_yaml may detect nested duplicates as ParseError, which is also acceptable
        let yaml = "outer:\n  inner: 1\n  inner: 2";
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(
                result,
                Err(CanonicalizeError::DuplicateKey { .. })
                    | Err(CanonicalizeError::ParseError { .. })
            ),
            "Should reject duplicate nested key 'inner' (via DuplicateKey or ParseError): {:?}",
            result
        );
        // Verify the error message mentions the duplicate key
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("inner") || msg.contains("duplicate"),
                "Error should mention duplicate: {}",
                msg
            );
        }
    }

    #[test]
    fn test_reject_duplicate_keys_different_values() {
        // Duplicate keys with completely different value types
        let yaml = "config: true\nconfig: some_string";
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(result, Err(CanonicalizeError::DuplicateKey { .. })),
            "Should reject duplicate key 'config': {:?}",
            result
        );
    }

    #[test]
    fn test_allow_same_key_different_levels() {
        // Same key name at different nesting levels is OK
        let yaml = "name: outer\nnested:\n  name: inner";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Same key at different levels should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_allow_unique_keys() {
        // All unique keys should work
        let yaml = "name: test\nversion: \"1.0.0\"\nkind: pack";
        let result = parse_yaml_strict(yaml);
        assert!(result.is_ok());
    }

    // ==================== Single Quote / Block Scalar Tests (P1 Fix) ====================

    #[test]
    fn test_ampersand_in_single_quoted_string() {
        // Single-quoted string with & should NOT be rejected as anchor
        let yaml = "text: 'this & that'";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Single-quoted ampersand should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_asterisk_in_single_quoted_string() {
        // Single-quoted string with * should NOT be rejected as alias
        let yaml = "pattern: '*.txt'";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Single-quoted asterisk should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_tag_in_quoted_string_allowed() {
        // !! inside a quoted string should be fine
        let yaml = r#"message: "Use !!binary for base64""#;
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Tag syntax in quoted string should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_quoted_key_with_special_chars() {
        // Quoted keys with special characters
        let yaml = r#""key:with:colons": value"#;
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Quoted key with colons should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_duplicate_quoted_keys() {
        // Duplicate keys even when quoted
        let yaml = r#""name": first
"name": second"#;
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(result, Err(CanonicalizeError::DuplicateKey { .. })),
            "Should reject duplicate quoted keys: {:?}",
            result
        );
    }

    // ==================== Flow Mapping Policy Tests ====================
    // Per SPEC: Packs SHOULD use block mappings. Flow mappings are parsed
    // but duplicate detection relies on serde_yaml (not pre-scan).

    #[test]
    fn test_flow_mapping_simple_allowed() {
        // Simple flow mappings without duplicates are allowed
        let yaml = "config: {a: 1, b: 2}";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Simple flow mapping should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_flow_mapping_duplicate_detected_by_serde() {
        // Flow mapping duplicates are detected by serde_yaml, not pre-scan
        // This is acceptable - duplicates are still rejected
        let yaml = "config: {a: 1, a: 2}";
        let result = parse_yaml_strict(yaml);
        // serde_yaml detects this as ParseError
        assert!(
            matches!(result, Err(CanonicalizeError::ParseError { .. })),
            "Flow mapping duplicates should be rejected (via serde_yaml): {:?}",
            result
        );
    }

    #[test]
    fn test_top_level_flow_mapping_duplicate() {
        // Top-level flow mapping with duplicates
        let yaml = "{a: 1, a: 2}";
        let result = parse_yaml_strict(yaml);
        // Detected by serde_yaml
        assert!(
            matches!(result, Err(CanonicalizeError::ParseError { .. })),
            "Top-level flow mapping duplicates should be rejected: {:?}",
            result
        );
    }

    #[test]
    fn test_complex_key_rejected() {
        // Complex keys (? syntax) should be rejected or cause parse error
        // This is not supported in pack YAML subset
        let yaml = "? [a, b]\n: value";
        let result = parse_yaml_strict(yaml);
        // This may parse but produces non-string key which is rejected
        assert!(
            result.is_err(),
            "Complex keys should be rejected: {:?}",
            result
        );
    }

    // ==================== List Item Scoping Tests ====================

    #[test]
    fn test_list_items_same_key_allowed() {
        // Different list items can have the same key (separate scopes)
        let yaml = "items:\n  - name: first\n  - name: second";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Same keys in different list items should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_list_item_duplicate_within_same_item() {
        // Duplicate keys within same list item should be rejected
        // May be detected by pre-scan (DuplicateKey) or serde_yaml (ParseError)
        let yaml = "items:\n  - name: first\n    name: second";
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(
                result,
                Err(CanonicalizeError::DuplicateKey { .. })
                    | Err(CanonicalizeError::ParseError { .. })
            ),
            "Duplicate keys within same list item should be rejected: {:?}",
            result
        );
    }

    #[test]
    fn test_top_level_sequence_same_keys() {
        // Top-level sequence: `- a: 1\n- a: 2` should be valid
        let yaml = "- a: 1\n- a: 2";
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Top-level sequence items with same keys should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_top_level_sequence_duplicate_in_item() {
        // Top-level sequence with duplicate in one item: invalid
        // May be detected by pre-scan (DuplicateKey) or serde_yaml (ParseError)
        let yaml = "- a: 1\n  a: 2";
        let result = parse_yaml_strict(yaml);
        assert!(
            matches!(
                result,
                Err(CanonicalizeError::DuplicateKey { .. })
                    | Err(CanonicalizeError::ParseError { .. })
            ),
            "Duplicate within top-level sequence item should be rejected: {:?}",
            result
        );
    }

    #[test]
    fn test_nested_list_with_mappings() {
        // Nested list items with multiple keys each
        let yaml = r#"rules:
  - id: rule1
    name: First Rule
  - id: rule2
    name: Second Rule"#;
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Nested list with multiple keys per item should work: {:?}",
            result
        );
    }

    #[test]
    fn test_deeply_nested_list_same_keys() {
        // Deeply nested: each list item has same structure
        let yaml = r#"outer:
  inner:
    - key: val1
      other: a
    - key: val2
      other: b"#;
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Deeply nested list items with same keys should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn test_mixed_sequence_and_mapping() {
        // Mixed: sequence items followed by regular mapping
        let yaml = r#"items:
  - name: item1
  - name: item2
metadata:
  name: should_not_conflict"#;
        let result = parse_yaml_strict(yaml);
        assert!(
            result.is_ok(),
            "Sequence keys should not conflict with sibling mapping keys: {:?}",
            result
        );
    }
}
