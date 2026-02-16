//! Strict JSON parsing with duplicate key rejection.
//!
//! Standard JSON parsers (including serde_json) accept duplicate keys with
//! "last key wins" semantics. This is a security risk for mandate evidence
//! where different parsers in the pipeline could interpret the same JSON
//! differently.
//!
//! # Security Rationale
//!
//! ```text
//! Attacker crafts: {"mandate_id": "legit", "mandate_id": "evil"}
//! Parser A sees:   mandate_id = "legit" (first wins)
//! Parser B sees:   mandate_id = "evil"  (last wins)
//! ```
//!
//! By rejecting duplicates at ingest, we ensure all downstream code sees
//! the same semantics.
//!
//! # Normative Behavior (SPEC-Mandate-v1)
//!
//! Object member names MUST be compared after JSON string unescaping:
//!
//! - Unicode escapes (`\uXXXX`) are decoded to actual characters
//! - Surrogate pairs are combined into Unicode scalars (U+10000+)
//! - Standard escapes (`\n`, `\t`, `\/`, `\\`, `\"`) are decoded
//!
//! This ensures `"a"` and `"\u0061"` are correctly detected as duplicate keys.
//!
//! **Note:** Duplicates are detected after JSON escape decoding, not Unicode
//! normalization (NFC/NFKC). So `"\u00E9"` and `"e\u0301"` are different
//! strings, even if they render identically.
//!
//! # DoS Protection
//!
//! This validator enforces limits to prevent resource exhaustion:
//! - Max nesting depth: 64 levels
//! - Max keys per object: 10,000
//! - Max string length: 1MB
//!
//! # Usage
//!
//! ```
//! use assay_evidence::json_strict::from_str_strict;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Data { key: String }
//!
//! // Rejects: {"key": "a", "key": "b"}
//! let result = from_str_strict::<Data>(r#"{"key": "a", "key": "b"}"#);
//! assert!(result.is_err());
//!
//! // Also rejects: {"a": 1, "\u0061": 2} (same key after decoding)
//! let result = from_str_strict::<serde_json::Value>(r#"{"a": 1, "\u0061": 2}"#);
//! assert!(result.is_err());
//! ```

mod dupkeys;
mod errors;
mod json_strict_internal;
mod scan;

pub use errors::{StrictJsonError, MAX_KEYS_PER_OBJECT, MAX_NESTING_DEPTH, MAX_STRING_LENGTH};
use serde::de::DeserializeOwned;

/// Parse JSON with strict duplicate key rejection.
///
/// Scans the JSON for duplicate keys at any nesting level before deserializing.
/// This ensures semantic consistency across different JSON parsers.
pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
    json_strict_internal::run::from_str_strict_impl(s)
}

/// Validate JSON string for security issues without deserializing.
///
/// Checks:
/// - Duplicate keys at any nesting level
/// - Lone surrogates in unicode escapes
pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError> {
    json_strict_internal::run::validate_json_strict_impl(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        key: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct NestedStruct {
        outer: Inner,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Inner {
        key: String,
    }

    // === B1: Duplicate Key Tests ===

    #[test]
    fn test_rejects_top_level_duplicate() {
        let json = r#"{"key": "first", "key": "second"}"#;
        let result = from_str_strict::<TestStruct>(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key" && path == "/"
        ));
    }

    #[test]
    fn test_rejects_nested_duplicate() {
        let json = r#"{"outer": {"key": "a", "key": "b"}}"#;
        let result = from_str_strict::<NestedStruct>(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key" && path == "/outer"
        ));
    }

    #[test]
    fn test_rejects_deeply_nested_duplicate() {
        let json = r#"{"data": {"scope": {"tools": ["a"], "tools": ["b"]}}}"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "tools" && path == "/data/scope"
        ));
    }

    #[test]
    fn test_accepts_same_key_different_objects() {
        let json = r#"{"a": {"key": "1"}, "b": {"key": "2"}}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_unicode_escape_duplicate() {
        let json = r#"{"a": 1, "\u0061": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a"),
            "Unicode escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_mixed_escape_duplicate() {
        let json = r#"{"Hello": 1, "\u0048\u0065\u006c\u006c\u006f": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "Hello"),
            "Fully escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_partial_escape_duplicate() {
        let json = r#"{"key": 1, "k\u0065y": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "key"),
            "Partially escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_surrogate_pair_duplicate() {
        let json = r#"{"😀": 1, "\uD83D\uDE00": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "😀"),
            "Surrogate pair key must be detected as duplicate of direct UTF-8: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_escaped_solidus_duplicate() {
        let json = r#"{"a/b": 1, "a\/b": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a/b"),
            "Escaped solidus key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_escaped_quote_duplicate() {
        let json = r#"{"a\\b": 1, "a\u005Cb": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a\\b"),
            "Escaped backslash key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_accepts_valid_json() {
        let json = r#"{"key": "value"}"#;
        let result: TestStruct = from_str_strict(json).unwrap();
        assert_eq!(result.key, "value");
    }

    // === B2: Lone Surrogate Tests ===

    #[test]
    fn test_rejects_lone_high_surrogate() {
        let json = r#"{"key": "\uD800"}"#;
        let result = validate_json_strict(json);
        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_rejects_lone_low_surrogate() {
        let json = r#"{"key": "\uDC00"}"#;
        let result = validate_json_strict(json);
        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_accepts_valid_surrogate_pair() {
        let json = r#"{"key": "\uD83D\uDE00"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_reversed_surrogate_pair() {
        let json = r#"{"key": "\uDC00\uD800"}"#;
        let result = validate_json_strict(json);
        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_accepts_non_surrogate_unicode() {
        let json = r#"{"key": "\u0041"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    // === Edge Cases ===

    #[test]
    fn test_array_with_objects() {
        let json = r#"[{"key": "a", "key": "b"}]"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, .. })
            if key == "key"
        ));
    }

    #[test]
    fn test_complex_nested_structure() {
        let json = r#"{
            "manifest": {"version": "1.0"},
            "events": [
                {"type": "test", "data": {"a": 1}},
                {"type": "test", "data": {"b": 2}}
            ]
        }"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_objects_and_arrays() {
        let json = r#"{"empty_obj": {}, "empty_arr": []}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_signature_duplicate_key_attack() {
        let json = r#"{"signature": {"key_id": "legit", "key_id": "evil"}}"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key_id" && path == "/signature"
        ));
    }

    // === DoS Protection Tests ===

    #[test]
    fn test_dos_nesting_depth_limit() {
        let deep_open = "{\"a\":".repeat(65);
        let deep_close = "}".repeat(65);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::NestingTooDeep { depth: 65 })),
            "Expected NestingTooDeep error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_dos_nesting_at_limit_ok() {
        let deep_open = "{\"a\":".repeat(64);
        let deep_close = "}".repeat(64);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "64 levels of nesting should be allowed");
    }

    #[test]
    fn test_dos_array_nesting_counts() {
        let deep_open = "[".repeat(65);
        let deep_close = "]".repeat(65);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::NestingTooDeep { .. })),
            "Array nesting should count towards depth limit"
        );
    }

    // === Edge Cases: Whitespace & Escapes ===

    #[test]
    fn test_crlf_in_string_accepted() {
        let json = r#"{"key": "line1\r\nline2"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok(), "Escaped CRLF in string should be accepted");
    }

    #[test]
    fn test_whitespace_between_tokens() {
        let json = "{ \t\n\r\"key\" \t:\r\n \"value\" \t}";
        let result = validate_json_strict(json);
        assert!(
            result.is_ok(),
            "Whitespace between tokens should be accepted"
        );
    }

    #[test]
    fn test_many_unicode_escapes_in_string() {
        let escapes = "\\u0061".repeat(1000);
        let json = format!(r#"{{"key": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "Many unicode escapes should be handled");
    }

    #[test]
    fn test_string_length_limit_on_decoded_content() {
        let escapes = "\\u0061".repeat(10000);
        let json = format!(r#"{{"key": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "10k escaped chars should be under limit");
    }

    #[test]
    fn test_surrogate_pair_counts_as_one_decoded_char() {
        // \uD83D\uDE00 = 😀; each pair produces one decoded char for limit purposes.
        let pairs = "\\uD83D\\uDE00".repeat(1000);
        let json = format!(r#"{{"key": "{}"}}"#, pairs);
        let result = validate_json_strict(&json);
        assert!(
            result.is_ok(),
            "1000 surrogate pairs (1000 decoded chars) should be under limit"
        );
    }

    #[test]
    fn test_mixed_escapes_in_key() {
        let json = r#"{"a\tb\nc\\d\"e": "value"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok(), "Mixed escapes in key should be accepted");
    }

    #[test]
    fn test_all_standard_escapes() {
        let json = r#"{"key": "\"\\/\b\f\n\r\t"}"#;
        let result = validate_json_strict(json);
        assert!(
            result.is_ok(),
            "All standard JSON escapes should be accepted"
        );
    }

    // === Limit boundary tests ===

    #[test]
    fn test_string_length_at_limit_ok() {
        // Exactly MAX_STRING_LENGTH decoded chars at boundary (closing quote does not count).
        let value = "a".repeat(MAX_STRING_LENGTH);
        let json = format!(r#"{{"k": "{}"}}"#, value);
        let result = validate_json_strict(&json);
        assert!(
            result.is_ok(),
            "String of exactly MAX_STRING_LENGTH decoded chars should be accepted"
        );
    }

    #[test]
    fn test_string_length_over_limit_rejected() {
        let value = "a".repeat(MAX_STRING_LENGTH + 1);
        let json = format!(r#"{{"k": "{}"}}"#, value);
        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::StringTooLong { .. })),
            "String exceeding MAX_STRING_LENGTH decoded chars must be rejected, got: {:?}",
            result
        );
    }

    #[test]
    fn test_string_length_at_limit_ok_via_escapes() {
        // Decode path: \u0061 = 'a'; exactly at limit must be accepted.
        let escapes = "\\u0061".repeat(MAX_STRING_LENGTH);
        let json = format!(r#"{{"k": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(
            result.is_ok(),
            "String of exactly MAX_STRING_LENGTH decoded chars (via escapes) should be accepted"
        );
    }

    #[test]
    fn test_string_length_over_limit_rejected_via_escapes() {
        // Decode path: \u0061 = 'a'; over limit must fail. Catches regressions where
        // char_count is incremented per input iteration instead of per output char.
        let escapes = "\\u0061".repeat(MAX_STRING_LENGTH + 1);
        let json = format!(r#"{{"k": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::StringTooLong { .. })),
            "String exceeding MAX_STRING_LENGTH decoded chars (via escapes) must be rejected, got: {:?}",
            result
        );
    }

    #[test]
    fn test_keys_at_limit_ok() {
        let mut pairs: Vec<String> = Vec::with_capacity(MAX_KEYS_PER_OBJECT);
        for i in 0..MAX_KEYS_PER_OBJECT {
            pairs.push(format!(r#""k{}": {}"#, i, i));
        }
        let json = format!("{{{}}}", pairs.join(","));
        let result = validate_json_strict(&json);
        assert!(
            result.is_ok(),
            "Object with exactly MAX_KEYS_PER_OBJECT keys should be accepted"
        );
    }

    #[test]
    fn test_keys_over_limit_rejected() {
        let count = MAX_KEYS_PER_OBJECT + 1;
        let mut pairs: Vec<String> = Vec::with_capacity(count);
        for i in 0..count {
            pairs.push(format!(r#""k{}": {}"#, i, i));
        }
        let json = format!("{{{}}}", pairs.join(","));
        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::TooManyKeys { .. })),
            "Object exceeding MAX_KEYS_PER_OBJECT must be rejected, got: {:?}",
            result
        );
    }
}
