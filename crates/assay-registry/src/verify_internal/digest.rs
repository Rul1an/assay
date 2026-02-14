//! Digest boundary for verify split.
//!
//! Contract target:
//! - digest compute/compare helpers only
//! - no DSSE signature verification
//! - no policy decisions

use crate::canonicalize::{
    compute_canonical_digest, parse_yaml_strict, to_canonical_jcs_bytes, CanonicalizeError,
};
use crate::digest::{compute_canonical_or_raw_digest, sha256_hex_bytes};
use crate::error::{RegistryError, RegistryResult};

pub(crate) fn verify_digest_impl(content: &str, expected: &str) -> RegistryResult<()> {
    let computed = compute_digest_impl(content);
    if computed != expected {
        return Err(RegistryError::DigestMismatch {
            name: "pack".to_string(),
            version: "unknown".to_string(),
            expected: expected.to_string(),
            actual: computed,
        });
    }
    Ok(())
}

pub(crate) fn compute_digest_impl(content: &str) -> String {
    compute_canonical_or_raw_digest(content, |e| {
        tracing::warn!(
            error = %e,
            "canonical digest failed, falling back to raw digest"
        );
    })
}

pub(crate) fn compute_digest_strict_impl(content: &str) -> Result<String, CanonicalizeError> {
    compute_canonical_digest(content)
}

pub(crate) fn compute_digest_raw_impl(content: &str) -> String {
    sha256_hex_bytes(content.as_bytes())
}

pub(crate) fn canonicalize_for_dsse_impl(content: &str) -> RegistryResult<Vec<u8>> {
    let json_value = parse_yaml_strict(content).map_err(|e| RegistryError::InvalidResponse {
        message: format!("failed to parse YAML for signature verification: {}", e),
    })?;

    to_canonical_jcs_bytes(&json_value).map_err(|e| RegistryError::InvalidResponse {
        message: format!("failed to canonicalize for signature verification: {}", e),
    })
}
