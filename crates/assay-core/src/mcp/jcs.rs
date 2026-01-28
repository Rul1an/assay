//! JSON Canonicalization Scheme (RFC 8785) for tool signing.
//!
//! Provides deterministic JSON serialization for cryptographic operations.
//! Uses `serde_jcs` which guarantees:
//!
//! - Lexicographic key ordering (per JCS sorting rules)
//! - No whitespace between tokens
//! - UTF-8 encoding
//! - Numbers serialized per ECMAScript/IEEE 754 constraints
//! - Unicode preserved as-is (no normalization)

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
/// - Unicode preserved as-is
///
/// # Errors
///
/// Returns error if serialization fails (e.g., lone surrogates in strings).
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_jcs::to_vec(value).context("JCS canonicalization failed")
}

/// Serialize to JCS Canonical JSON string.
pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
    serde_jcs::to_string(value).context("JCS canonicalization failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_jcs_key_ordering() {
        let input = json!({"z": 3, "b": 2, "a": 1});
        let canonical = to_string(&input).unwrap();
        assert_eq!(canonical, r#"{"a":1,"b":2,"z":3}"#);
    }

    #[test]
    fn test_jcs_nested_ordering() {
        let input = json!({"outer": {"z": 1, "a": 2}, "first": true});
        let canonical = to_string(&input).unwrap();
        assert_eq!(canonical, r#"{"first":true,"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_jcs_no_whitespace() {
        let input = json!({"key": "value", "array": [1, 2, 3]});
        let canonical = to_string(&input).unwrap();
        assert!(!canonical.contains(' '));
        assert!(!canonical.contains('\n'));
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

    #[test]
    fn test_jcs_unicode_preserved() {
        let input = json!({"emoji": "🔒", "chinese": "中文"});
        let bytes = to_vec(&input).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("🔒"));
        assert!(s.contains("中文"));
    }
}
