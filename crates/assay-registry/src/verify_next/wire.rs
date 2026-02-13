//! Wire-level parsing boundary for Step-2 split.
//!
//! Contract target:
//! - parsing and shape checks only
//! - no policy decisions
//! - no cryptographic verification

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::canonicalize::{parse_yaml_strict, to_canonical_jcs_bytes};
use crate::error::RegistryResult;
use crate::types::DsseEnvelope;

use super::errors_next;

pub(super) fn canonicalize_for_dsse_impl(content: &str) -> RegistryResult<Vec<u8>> {
    let json_value = parse_yaml_strict(content).map_err(|e| {
        errors_next::invalid_response(format!(
            "failed to parse YAML for signature verification: {}",
            e
        ))
    })?;

    to_canonical_jcs_bytes(&json_value).map_err(|e| {
        errors_next::invalid_response(format!(
            "failed to canonicalize for signature verification: {}",
            e
        ))
    })
}

pub(super) fn parse_dsse_envelope_impl(b64: &str) -> RegistryResult<DsseEnvelope> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| errors_next::signature_invalid(format!("invalid base64 envelope: {}", e)))?;

    serde_json::from_slice(&bytes)
        .map_err(|e| errors_next::signature_invalid(format!("invalid DSSE envelope: {}", e)))
}
