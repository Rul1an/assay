//! Digest parsing/compare boundary for Step-2 split.
//!
//! Contract target:
//! - digest-only helpers
//! - no DSSE verification
//! - no policy decisions

use crate::canonicalize::{compute_canonical_digest, CanonicalizeError};
use crate::digest::{compute_canonical_or_raw_digest, sha256_hex_bytes};
use crate::error::RegistryResult;

use super::errors_next;

pub(super) fn verify_digest_impl(content: &str, expected: &str) -> RegistryResult<()> {
    let computed = compute_digest_impl(content);
    if computed != expected {
        return Err(errors_next::digest_mismatch(expected, computed));
    }
    Ok(())
}

pub(super) fn compute_digest_impl(content: &str) -> String {
    compute_canonical_or_raw_digest(content, |e| {
        tracing::warn!(
            error = %e,
            "canonical digest failed, falling back to raw digest"
        );
    })
}

pub(super) fn compute_digest_strict_impl(content: &str) -> Result<String, CanonicalizeError> {
    compute_canonical_digest(content)
}

#[allow(deprecated)]
pub(super) fn compute_digest_raw_impl(content: &str) -> String {
    sha256_hex_bytes(content.as_bytes())
}
