//! Key helper boundary for verify split.
//!
//! Contract target:
//! - key-id helpers only
//! - no policy decisions

use ed25519_dalek::VerifyingKey;

use crate::digest::sha256_hex_bytes;
use crate::error::{RegistryError, RegistryResult};

pub(crate) fn compute_key_id_impl(spki_bytes: &[u8]) -> String {
    sha256_hex_bytes(spki_bytes)
}

pub(crate) fn compute_key_id_from_key_impl(key: &VerifyingKey) -> RegistryResult<String> {
    use pkcs8::EncodePublicKey;
    let doc = key.to_public_key_der().map_err(|e| RegistryError::Config {
        message: format!("failed to encode public key: {}", e),
    })?;
    Ok(compute_key_id_impl(doc.as_bytes()))
}
