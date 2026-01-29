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
//! - Duplicate keys
//! - Floats (only integers allowed)
//! - Integers outside safe range (> 2^53)
//!
//! # DoS Limits (§12.4)
//!
//! - Max depth: 50
//! - Max keys per mapping: 10,000
//! - Max string length: 1MB
//! - Max total size: 10MB

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::error::{RegistryError, RegistryResult};

/// Maximum nesting depth for YAML structures.
pub const MAX_DEPTH: usize = 50;

/// Maximum number of keys in a single mapping.
pub const MAX_KEYS_PER_MAPPING: usize = 10_000;

/// Maximum string length (1MB).
pub const MAX_STRING_LENGTH: usize = 1_024 * 1_024;

/// Maximum total YAML size (10MB).
pub const MAX_TOTAL_SIZE: usize = 10 * 1_024 * 1_024;

/// Maximum safe integer value (2^53 for JSON compatibility).
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_992; // 2^53

/// Minimum safe integer value (-2^53 for JSON compatibility).
pub const MIN_SAFE_INTEGER: i64 = -9_007_199_254_740_992; // -2^53

/// Errors specific to canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalizeError {
    /// YAML contains anchors (forbidden).
    AnchorFound { position: String },

    /// YAML contains aliases (forbidden).
    AliasFound { position: String },

    /// YAML contains tags (forbidden).
    TagFound { tag: String },

    /// YAML contains multiple documents (forbidden).
    MultiDocumentFound,

    /// YAML contains duplicate keys (forbidden).
    DuplicateKey { key: String },

    /// YAML contains float values (forbidden).
    FloatNotAllowed { value: String },

    /// Integer outside safe range.
    IntegerOutOfRange { value: i64 },

    /// Nesting too deep.
    MaxDepthExceeded { depth: usize },

    /// Too many keys in mapping.
    MaxKeysExceeded { count: usize },

    /// String too long.
    StringTooLong { length: usize },

    /// Input too large.
    InputTooLarge { size: usize },

    /// YAML parse error.
    ParseError { message: String },

    /// JSON serialization error.
    SerializeError { message: String },
}

impl std::fmt::Display for CanonicalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnchorFound { position } => write!(f, "YAML anchor found at {}", position),
            Self::AliasFound { position } => write!(f, "YAML alias found at {}", position),
            Self::TagFound { tag } => write!(f, "YAML tag not allowed: {}", tag),
            Self::MultiDocumentFound => write!(f, "multi-document YAML not allowed"),
            Self::DuplicateKey { key } => write!(f, "duplicate key: {}", key),
            Self::FloatNotAllowed { value } => write!(f, "float values not allowed: {}", value),
            Self::IntegerOutOfRange { value } => {
                write!(f, "integer {} out of safe range (±2^53)", value)
            }
            Self::MaxDepthExceeded { depth } => {
                write!(f, "nesting depth {} exceeds limit {}", depth, MAX_DEPTH)
            }
            Self::MaxKeysExceeded { count } => write!(
                f,
                "mapping has {} keys, exceeds limit {}",
                count, MAX_KEYS_PER_MAPPING
            ),
            Self::StringTooLong { length } => write!(
                f,
                "string length {} exceeds limit {}",
                length, MAX_STRING_LENGTH
            ),
            Self::InputTooLarge { size } => {
                write!(f, "input size {} exceeds limit {}", size, MAX_TOTAL_SIZE)
            }
            Self::ParseError { message } => write!(f, "YAML parse error: {}", message),
            Self::SerializeError { message } => write!(f, "JSON serialize error: {}", message),
        }
    }
}

impl std::error::Error for CanonicalizeError {}

impl From<CanonicalizeError> for RegistryError {
    fn from(err: CanonicalizeError) -> Self {
        RegistryError::InvalidResponse {
            message: format!("canonicalization failed: {}", err),
        }
    }
}

/// Result type for canonicalization operations.
pub type CanonicalizeResult<T> = Result<T, CanonicalizeError>;

/// Parse YAML with strict validation per SPEC §6.1.
///
/// Validates:
/// - No anchors/aliases
/// - No tags
/// - No multi-document
/// - No duplicate keys
/// - No floats
/// - Integers within safe range
/// - DoS limits (depth, keys, string length, total size)
pub fn parse_yaml_strict(content: &str) -> CanonicalizeResult<JsonValue> {
    // Pre-check: input size
    if content.len() > MAX_TOTAL_SIZE {
        return Err(CanonicalizeError::InputTooLarge {
            size: content.len(),
        });
    }

    // Pre-scan for forbidden patterns
    pre_scan_yaml(content)?;

    // Parse YAML to intermediate value
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| CanonicalizeError::ParseError {
            message: e.to_string(),
        })?;

    // Convert to JSON and validate
    let json_value = yaml_to_json(&yaml_value, 0)?;

    Ok(json_value)
}

/// Pre-scan YAML for forbidden patterns.
///
/// This is a fast check before full parsing to reject obviously invalid input.
fn pre_scan_yaml(content: &str) -> CanonicalizeResult<()> {
    let mut in_string = false;
    let mut escape_next = false;
    let mut prev_char = '\0';
    let mut line_start = true;

    for (i, c) in content.char_indices() {
        if escape_next {
            escape_next = false;
            prev_char = c;
            continue;
        }

        match c {
            // Track string state (simplified - doesn't handle all edge cases)
            '"' if !in_string => in_string = true,
            '"' if in_string => in_string = false,
            '\\' if in_string => escape_next = true,

            // Check for anchors: &name (not in string, not &amp; entity)
            '&' if !in_string => {
                // Look ahead for valid anchor name char
                let rest = &content[i..];
                if rest.len() > 1 {
                    let next = rest.chars().nth(1);
                    if let Some(nc) = next {
                        if nc.is_alphanumeric() || nc == '_' {
                            return Err(CanonicalizeError::AnchorFound {
                                position: format!("byte {}", i),
                            });
                        }
                    }
                }
            }

            // Check for aliases: *name (not in string)
            '*' if !in_string => {
                let rest = &content[i..];
                if rest.len() > 1 {
                    let next = rest.chars().nth(1);
                    if let Some(nc) = next {
                        if nc.is_alphanumeric() || nc == '_' {
                            return Err(CanonicalizeError::AliasFound {
                                position: format!("byte {}", i),
                            });
                        }
                    }
                }
            }

            // Check for tags: !! or !<
            '!' if !in_string => {
                let rest = &content[i..];
                if rest.starts_with("!!") || rest.starts_with("!<") {
                    // Extract tag for error message
                    let tag_end = rest
                        .find(|c: char| c.is_whitespace() || c == ':' || c == '\n')
                        .unwrap_or(rest.len().min(20));
                    return Err(CanonicalizeError::TagFound {
                        tag: rest[..tag_end].to_string(),
                    });
                }
            }

            // Check for multi-document: --- at line start
            '-' if !in_string && line_start => {
                let rest = &content[i..];
                if rest.starts_with("---") {
                    // Check if it's actually a document separator
                    let after = rest.get(3..4);
                    if after.is_none()
                        || after == Some("\n")
                        || after == Some(" ")
                        || after == Some("\r")
                    {
                        return Err(CanonicalizeError::MultiDocumentFound);
                    }
                }
            }

            '\n' => line_start = true,
            _ if !c.is_whitespace() => line_start = false,
            _ => {}
        }

        prev_char = c;
    }

    // Suppress unused variable warning
    let _ = prev_char;

    Ok(())
}

/// Convert YAML value to JSON value with validation.
fn yaml_to_json(yaml: &serde_yaml::Value, depth: usize) -> CanonicalizeResult<JsonValue> {
    // Check depth limit
    if depth > MAX_DEPTH {
        return Err(CanonicalizeError::MaxDepthExceeded { depth });
    }

    match yaml {
        serde_yaml::Value::Null => Ok(JsonValue::Null),

        serde_yaml::Value::Bool(b) => Ok(JsonValue::Bool(*b)),

        serde_yaml::Value::Number(n) => {
            // Check for float
            if n.is_f64() {
                return Err(CanonicalizeError::FloatNotAllowed {
                    value: n.to_string(),
                });
            }

            // Check integer range
            if let Some(i) = n.as_i64() {
                if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&i) {
                    return Err(CanonicalizeError::IntegerOutOfRange { value: i });
                }
                Ok(JsonValue::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                if u > MAX_SAFE_INTEGER as u64 {
                    return Err(CanonicalizeError::IntegerOutOfRange { value: u as i64 });
                }
                Ok(JsonValue::Number(serde_json::Number::from(u)))
            } else {
                Err(CanonicalizeError::FloatNotAllowed {
                    value: n.to_string(),
                })
            }
        }

        serde_yaml::Value::String(s) => {
            // Check string length
            if s.len() > MAX_STRING_LENGTH {
                return Err(CanonicalizeError::StringTooLong { length: s.len() });
            }
            Ok(JsonValue::String(s.clone()))
        }

        serde_yaml::Value::Sequence(seq) => {
            let items: CanonicalizeResult<Vec<JsonValue>> = seq
                .iter()
                .map(|item| yaml_to_json(item, depth + 1))
                .collect();
            Ok(JsonValue::Array(items?))
        }

        serde_yaml::Value::Mapping(map) => {
            // Check key count
            if map.len() > MAX_KEYS_PER_MAPPING {
                return Err(CanonicalizeError::MaxKeysExceeded { count: map.len() });
            }

            let mut json_map = serde_json::Map::new();
            let mut seen_keys = std::collections::HashSet::new();

            for (key, value) in map {
                // Keys must be strings
                let key_str = match key {
                    serde_yaml::Value::String(s) => s.clone(),
                    _ => {
                        return Err(CanonicalizeError::ParseError {
                            message: format!("non-string key: {:?}", key),
                        })
                    }
                };

                // Check for duplicate keys
                if !seen_keys.insert(key_str.clone()) {
                    return Err(CanonicalizeError::DuplicateKey { key: key_str });
                }

                let json_value = yaml_to_json(value, depth + 1)?;
                json_map.insert(key_str, json_value);
            }

            Ok(JsonValue::Object(json_map))
        }

        // Tagged values are not allowed
        serde_yaml::Value::Tagged(tagged) => Err(CanonicalizeError::TagFound {
            tag: format!("{:?}", tagged.tag),
        }),
    }
}

/// Convert a JSON value to JCS (JSON Canonicalization Scheme) bytes.
///
/// JCS (RFC 8785) produces deterministic JSON output by:
/// - Sorting object keys lexicographically by UTF-16 code units
/// - No whitespace
/// - Specific number formatting
pub fn to_canonical_jcs_bytes(value: &JsonValue) -> CanonicalizeResult<Vec<u8>> {
    serde_jcs::to_vec(value).map_err(|e| CanonicalizeError::SerializeError {
        message: e.to_string(),
    })
}

/// Compute canonical digest of YAML content.
///
/// Process:
/// 1. Parse YAML with strict validation
/// 2. Convert to JSON
/// 3. Serialize to JCS (RFC 8785)
/// 4. SHA-256 hash
/// 5. Format as `sha256:{hex}`
pub fn compute_canonical_digest(content: &str) -> CanonicalizeResult<String> {
    let json_value = parse_yaml_strict(content)?;
    let jcs_bytes = to_canonical_jcs_bytes(&json_value)?;
    let hash = Sha256::digest(&jcs_bytes);
    Ok(format!("sha256:{:x}", hash))
}

/// Compute canonical digest, returning RegistryResult for API compatibility.
pub fn compute_canonical_digest_result(content: &str) -> RegistryResult<String> {
    compute_canonical_digest(content).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
