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
