//! Integrity boundary scaffold for cache split.
//!
//! Planned ownership (Step2+):
//! - digest/signature verification helpers
//! - corruption handling helpers

use crate::error::{RegistryError, RegistryResult};
use crate::types::DsseEnvelope;

pub(crate) fn parse_signature_impl(b64: &str) -> RegistryResult<DsseEnvelope> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let bytes = BASE64.decode(b64).map_err(|e| RegistryError::Cache {
        message: format!("invalid base64 signature: {}", e),
    })?;

    serde_json::from_slice(&bytes).map_err(|e| RegistryError::Cache {
        message: format!("invalid DSSE envelope: {}", e),
    })
}
