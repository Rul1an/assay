//! JCS (JSON Canonicalization Scheme) helpers.

use serde_json::Value as JsonValue;

use super::errors::{CanonicalizeError, CanonicalizeResult};

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
