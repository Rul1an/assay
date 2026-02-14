//! Wire-format boundary for verify split.
//!
//! Contract target:
//! - envelope wire parsing/shape checks only
//! - no policy decisions
//! - no key trust decisions

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::error::{RegistryError, RegistryResult};
use crate::types::DsseEnvelope;

pub(crate) fn parse_dsse_envelope_impl(b64: &str) -> RegistryResult<DsseEnvelope> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| RegistryError::SignatureInvalid {
            reason: format!("invalid base64 envelope: {}", e),
        })?;

    serde_json::from_slice(&bytes).map_err(|e| RegistryError::SignatureInvalid {
        reason: format!("invalid DSSE envelope: {}", e),
    })
}
