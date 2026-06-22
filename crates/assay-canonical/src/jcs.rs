//! RFC 8785 (JSON Canonicalization Scheme) bytes.
//!
//! A thin wrapper over `serde_jcs` so the rest of the crate (and its callers) go through one place.
//! `serde_jcs` guarantees lexicographic object-key ordering, no insignificant whitespace, UTF-8, and
//! IEEE 754 number normalization. It does NOT sort array elements — arrays keep their emit order.

use serde::Serialize;

use crate::Error;

/// Serialize `value` to RFC 8785 (JCS) canonical JSON bytes.
///
/// ```
/// let bytes = assay_canonical::jcs::to_vec(&serde_json::json!({"b": 2, "a": 1})).unwrap();
/// assert_eq!(bytes, br#"{"a":1,"b":2}"#);
/// ```
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    serde_jcs::to_vec(value).map_err(|e| Error::Canonicalize(e.to_string()))
}

/// Serialize `value` to an RFC 8785 (JCS) canonical JSON string.
///
/// ```
/// let s = assay_canonical::jcs::to_string(&serde_json::json!({"z": 1, "a": 2})).unwrap();
/// assert_eq!(s, r#"{"a":2,"z":1}"#);
/// ```
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error> {
    serde_jcs::to_string(value).map_err(|e| Error::Canonicalize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_object_keys_not_arrays() {
        // Keys sorted; the array stays in emit order.
        let s = to_string(&json!({"z": [3, 1, 2], "a": 1})).unwrap();
        assert_eq!(s, r#"{"a":1,"z":[3,1,2]}"#);
    }

    #[test]
    fn matches_python_reference_bytes_over_the_fixture_domain() {
        // The Python reference canonical() is json.dumps(sort_keys=True, separators=(",",":")).
        // Over the goldens' value domain (ASCII strings + integers) serde_jcs (full RFC 8785) is
        // byte-identical, which is what makes the goldens a valid Rust/Python parity oracle.
        let normalized = json!({
            "dropped_keys": ["AWS_SECRET", "TOKEN"],
            "passed_keys": ["HOME", "PATH"],
        });
        assert_eq!(
            to_string(&normalized).unwrap(),
            r#"{"dropped_keys":["AWS_SECRET","TOKEN"],"passed_keys":["HOME","PATH"]}"#
        );
    }
}
