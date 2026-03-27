use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::VerifyingKey;

use crate::error::{RegistryError, RegistryResult};

/// Decode a Base64-encoded SPKI public key to VerifyingKey.
pub(in crate::trust) fn decode_verifying_key(b64: &str) -> RegistryResult<VerifyingKey> {
    use pkcs8::DecodePublicKey;

    let bytes = BASE64.decode(b64).map_err(|e| RegistryError::Config {
        message: format!("invalid base64 public key: {}", e),
    })?;

    VerifyingKey::from_public_key_der(&bytes).map_err(|e| RegistryError::Config {
        message: format!("invalid SPKI public key: {}", e),
    })
}

/// Decode Base64 public key bytes.
pub(in crate::trust) fn decode_public_key_bytes(b64: &str) -> RegistryResult<Vec<u8>> {
    BASE64.decode(b64).map_err(|e| RegistryError::Config {
        message: format!("invalid base64 public key: {}", e),
    })
}
