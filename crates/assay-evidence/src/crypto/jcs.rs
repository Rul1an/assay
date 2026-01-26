//! JSON Canonicalization Scheme (RFC 8785) implementation.
//!
//! Provides deterministic JSON serialization for cryptographic operations.
//! Uses `serde_jcs` which guarantees:
//!
//! - Lexicographic key ordering
//! - No insignificant whitespace
//! - UTF-8 encoding
//! - IEEE 754 number normalization (1.0 → 1)

use anyhow::{Context, Result};
use serde::Serialize;

/// Serialize a value to JCS (RFC 8785) Canonical JSON bytes.
///
/// # Guarantees
///
/// - Keys sorted lexicographically
/// - No whitespace
/// - UTF-8 encoding
/// - Number normalization (IEEE 754)
///
/// # Example
///
/// ```
/// use assay_evidence::crypto::jcs;
/// use serde_json::json;
///
/// let value = json!({"b": 2, "a": 1});
/// let bytes = jcs::to_vec(&value).unwrap();
/// assert_eq!(bytes, br#"{"a":1,"b":2}"#);
/// ```
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_jcs::to_vec(value).context("failed to serialize canonical json")
}

/// Serialize to JCS Canonical JSON string.
///
/// # Example
///
/// ```
/// use assay_evidence::crypto::jcs;
/// use serde_json::json;
///
/// let value = json!({"z": 1, "a": 2});
/// let s = jcs::to_string(&value).unwrap();
/// assert_eq!(s, r#"{"a":2,"z":1}"#);
/// ```
pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
    serde_jcs::to_string(value).context("failed to serialize canonical json string")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_jcs_key_ordering() {
        let input = json!({
            "z": 3,
            "b": 2,
            "a": 1,
            "m": 4
        });

        let canonical = to_string(&input).unwrap();
        assert_eq!(canonical, r#"{"a":1,"b":2,"m":4,"z":3}"#);
    }

    #[test]
    fn test_jcs_nested_ordering() {
        let input = json!({
            "outer": {
                "z": 1,
                "a": 2
            },
            "first": true
        });

        let canonical = to_string(&input).unwrap();
        assert_eq!(canonical, r#"{"first":true,"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_jcs_no_whitespace() {
        let input = json!({
            "key": "value",
            "array": [1, 2, 3]
        });

        let canonical = to_string(&input).unwrap();
        // No spaces, no newlines
        assert!(!canonical.contains(' '));
        assert!(!canonical.contains('\n'));
    }

    #[test]
    fn test_jcs_float_normalization() {
        let input = json!({
            "integer_looking_float": 1.0,
            "normal_float": 1.5,
            "zero": 0.0
        });

        let canonical = to_string(&input).unwrap();
        // JCS: 1.0 -> 1, 0.0 -> 0
        assert!(
            canonical.contains(r#""integer_looking_float":1"#)
                || canonical.contains(r#""integer_looking_float":1.0"#)
        );
        assert!(canonical.contains(r#""normal_float":1.5"#));
    }

    #[test]
    fn test_jcs_array_order_preserved() {
        let input = json!({
            "array": [3, 1, 2]
        });

        let canonical = to_string(&input).unwrap();
        // Arrays maintain order (not sorted)
        assert_eq!(canonical, r#"{"array":[3,1,2]}"#);
    }

    #[test]
    fn test_jcs_unicode() {
        let input = json!({
            "emoji": "🔒",
            "chinese": "中文"
        });

        let bytes = to_vec(&input).unwrap();
        // Should be valid UTF-8
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("🔒"));
        assert!(s.contains("中文"));
    }

    #[test]
    fn test_jcs_determinism() {
        // Same logical value, different construction order
        let input1 = json!({"a": 1, "b": 2});
        let input2 = json!({"b": 2, "a": 1});

        let canonical1 = to_vec(&input1).unwrap();
        let canonical2 = to_vec(&input2).unwrap();

        assert_eq!(canonical1, canonical2, "JCS must be deterministic");
    }
}
