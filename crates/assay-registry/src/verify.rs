//! Pack verification (digest and signature).
//!
//! Implements verification per SPEC-Pack-Registry-v1:
//! - Digest verification (SHA-256 of JCS-canonical content)
//! - DSSE signature verification (Ed25519 over PAE)

#[path = "verify_internal/mod.rs"]
mod verify_internal;

use ed25519_dalek::VerifyingKey;

use crate::canonicalize::CanonicalizeError;
use crate::error::RegistryResult;
use crate::trust::TrustStore;
use crate::types::FetchResult;

/// Payload type for pack definitions (DSSE-style binding).
pub const PAYLOAD_TYPE_PACK_V1: &str = "application/vnd.assay.pack+yaml;v=1";

/// Result of successful verification.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Whether the pack is signed.
    pub signed: bool,

    /// Key ID used for signing (if signed).
    pub key_id: Option<String>,

    /// Content digest.
    pub digest: String,
}

/// Verification options.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    /// Allow unsigned packs (for dev/testing).
    pub allow_unsigned: bool,

    /// Skip signature verification (verify digest only).
    pub skip_signature: bool,
}

impl VerifyOptions {
    /// Allow unsigned packs.
    pub fn allow_unsigned(mut self) -> Self {
        self.allow_unsigned = true;
        self
    }

    /// Skip signature verification.
    pub fn skip_signature(mut self) -> Self {
        self.skip_signature = true;
        self
    }
}

/// Verify a fetched pack.
///
/// # Verification Steps
///
/// 1. Compute SHA-256 digest of content
/// 2. Compare against `X-Pack-Digest` header (if present)
/// 3. If signed, verify DSSE signature over PAE
/// 4. Check key against trust store
///
/// # Arguments
///
/// * `result` - Fetch result from registry
/// * `trust_store` - Key trust store
/// * `options` - Verification options
pub fn verify_pack(
    result: &FetchResult,
    trust_store: &TrustStore,
    options: &VerifyOptions,
) -> RegistryResult<VerifyResult> {
    verify_internal::policy::verify_pack_impl(result, trust_store, options)
}

/// Verify content digest matches expected.
///
/// Uses canonical JCS digest per SPEC §6.2.
pub fn verify_digest(content: &str, expected: &str) -> RegistryResult<()> {
    verify_internal::digest::verify_digest_impl(content, expected)
}

/// Compute canonical digest of content per SPEC §6.2.
///
/// Process:
/// 1. Parse YAML with strict validation (§6.1)
/// 2. Convert to JSON
/// 3. Serialize to JCS (RFC 8785)
/// 4. SHA-256 hash
///
/// For content that may not be valid YAML, falls back to raw SHA-256.
pub fn compute_digest(content: &str) -> String {
    verify_internal::digest::compute_digest_impl(content)
}

/// Compute canonical digest, returning error on invalid YAML.
///
/// Use this when you need strict validation.
pub fn compute_digest_strict(content: &str) -> Result<String, CanonicalizeError> {
    verify_internal::digest::compute_digest_strict_impl(content)
}

/// Compute raw SHA-256 digest of content bytes.
///
/// **Deprecated**: Use `compute_digest` for canonical JCS digest per SPEC §6.2.
/// This function is only for backward compatibility with pre-v1.0.2 digests.
#[deprecated(since = "2.11.0", note = "use compute_digest for canonical JCS digest")]
pub fn compute_digest_raw(content: &str) -> String {
    verify_internal::digest::compute_digest_raw_impl(content)
}

/// Compute key ID from public key bytes (SPKI DER).
pub fn compute_key_id(spki_bytes: &[u8]) -> String {
    verify_internal::keys::compute_key_id_impl(spki_bytes)
}

/// Compute key ID from a VerifyingKey.
pub fn compute_key_id_from_key(key: &VerifyingKey) -> RegistryResult<String> {
    verify_internal::keys::compute_key_id_from_key_impl(key)
}
