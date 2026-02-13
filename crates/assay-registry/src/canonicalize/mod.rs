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
//! - Merge keys (`<<`)
//! - Duplicate keys (see [Duplicate key detection](#duplicate-key-detection))
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
//! Flow mapping duplicate keys are detected by serde_yaml during parsing,
//! not by the pre-scan. Both detection methods result in rejection.
//!
//! # Duplicate key detection
//!
//! Three layers enforce no duplicate keys:
//!
//! 1. **Pre-scan** (block mappings): Token-level on raw YAML lines. Keys are
//!    compared as extracted strings (quoted/unquoted). No Unicode normalization;
//!    `"a"` and `"\u0061"` are distinct at this layer (same as json_strict).
//! 2. **serde_yaml** (flow mappings): Parser rejects flow duplicates as `ParseError`.
//! 3. **yaml_to_json** (Mapping): After parsing, keys are compared via
//!    `serde_yaml::Value` equality (string identity). Again no NFC/NFKC.
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

#[cfg(test)]
mod tests;

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
