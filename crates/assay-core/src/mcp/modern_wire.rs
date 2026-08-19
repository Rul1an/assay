//! The pinned MCP 2026-07-28 vocabulary, without a transport or negotiation path.
//!
//! This module validates only shapes Assay needs to name. It does not advertise, negotiate, or
//! serve this revision. Request-metadata and result-type observations remain owned by `era`.

use super::era::{
    id_is_acceptable, observe_client_capabilities, observe_request_metadata, CapabilityObservation,
    RequestMetadata, RESULT_TYPE_SINCE,
};

/// The pinned revision this vocabulary models.
///
/// This names a schema vocabulary only. It does not indicate that any Assay transport accepts,
/// advertises, or serves the revision.
pub const MODERN_PROTOCOL_VERSION: &str = RESULT_TYPE_SINCE;

/// A value-free reason a modeled modern shape cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModernWireError {
    MissingProtocolVersion,
    MalformedProtocolVersion,
    UnexpectedProtocolVersion,
    MissingClientCapabilities,
    MalformedClientCapabilities,
    MissingResultType,
    MalformedResultType,
    MissingTtlMs,
    MalformedTtlMs,
    MissingCacheScope,
    MalformedCacheScope,
    MalformedCacheableResult,
    MalformedDiscoverRequest,
    MalformedDiscoverResult,
    MalformedUnsupportedProtocolVersionError,
}

/// Validate the per-request metadata required by the pinned revision.
///
/// This is a model-only validator. The server intentionally does not call it as an acceptance
/// path: it still refuses this revision before dispatch.
pub fn validate_request_metadata(raw: &serde_json::Value) -> Result<(), ModernWireError> {
    match observe_request_metadata(raw) {
        RequestMetadata::Absent => return Err(ModernWireError::MissingProtocolVersion),
        RequestMetadata::Malformed => return Err(ModernWireError::MalformedProtocolVersion),
        RequestMetadata::Present(version) if version != MODERN_PROTOCOL_VERSION => {
            return Err(ModernWireError::UnexpectedProtocolVersion);
        }
        RequestMetadata::Present(_) => {}
    }

    match observe_client_capabilities(raw) {
        Some(CapabilityObservation::CoreOnly | CapabilityObservation::ExtensionNotUnderstood) => {
            Ok(())
        }
        Some(CapabilityObservation::Malformed) => Err(ModernWireError::MalformedClientCapabilities),
        Some(CapabilityObservation::Absent) | None => {
            Err(ModernWireError::MissingClientCapabilities)
        }
    }
}

/// Validate the schema-required cache hint members without assigning a cache policy.
pub fn validate_cacheable_result(result: &serde_json::Value) -> Result<(), ModernWireError> {
    let Some(result) = result.as_object() else {
        return Err(ModernWireError::MalformedCacheableResult);
    };
    validate_result_type(result)?;
    match result.get("ttlMs") {
        None => return Err(ModernWireError::MissingTtlMs),
        Some(ttl) if !ttl.is_u64() => return Err(ModernWireError::MalformedTtlMs),
        Some(_) => {}
    }
    match result.get("cacheScope").and_then(serde_json::Value::as_str) {
        None if !result.contains_key("cacheScope") => Err(ModernWireError::MissingCacheScope),
        Some("private" | "public") => Ok(()),
        None | Some(_) => Err(ModernWireError::MalformedCacheScope),
    }
}

/// Validate the 2026 `server/discover` request shape without serving it.
pub fn validate_discover_request(raw: &serde_json::Value) -> Result<(), ModernWireError> {
    let Some(request) = raw.as_object() else {
        return Err(ModernWireError::MalformedDiscoverRequest);
    };
    if request.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0")
        || request.get("method").and_then(serde_json::Value::as_str) != Some("server/discover")
        || !request.get("id").is_some_and(id_is_acceptable)
        || !request
            .get("params")
            .is_some_and(serde_json::Value::is_object)
    {
        return Err(ModernWireError::MalformedDiscoverRequest);
    }
    validate_request_metadata(raw)
}

/// Validate the 2026 `server/discover` result shape without producing one on the wire.
pub fn validate_discover_result(raw: &serde_json::Value) -> Result<(), ModernWireError> {
    let Some(result) = raw.as_object() else {
        return Err(ModernWireError::MalformedDiscoverResult);
    };
    validate_cacheable_result(raw).map_err(|_| ModernWireError::MalformedDiscoverResult)?;
    if !result
        .get("capabilities")
        .is_some_and(serde_json::Value::is_object)
        || !result
            .get("supportedVersions")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|versions| versions.iter().all(serde_json::Value::is_string))
    {
        return Err(ModernWireError::MalformedDiscoverResult);
    }
    Ok(())
}

/// Validate the reserved 2026 unsupported-version error shape.
pub fn validate_unsupported_protocol_version_error(
    raw: &serde_json::Value,
) -> Result<(), ModernWireError> {
    let Some(error) = raw.get("error").and_then(serde_json::Value::as_object) else {
        return Err(ModernWireError::MalformedUnsupportedProtocolVersionError);
    };
    let valid_data = error
        .get("data")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|data| {
            data.get("requested")
                .is_some_and(serde_json::Value::is_string)
                && data
                    .get("supported")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|versions| versions.iter().all(serde_json::Value::is_string))
        });
    if raw.get("jsonrpc").and_then(serde_json::Value::as_str) == Some("2.0")
        && raw.get("id").is_none_or(id_is_acceptable)
        && error.get("code").and_then(serde_json::Value::as_i64) == Some(-32022)
        && error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some()
        && valid_data
    {
        Ok(())
    } else {
        Err(ModernWireError::MalformedUnsupportedProtocolVersionError)
    }
}

fn validate_result_type(
    result: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ModernWireError> {
    match result.get("resultType") {
        None => Err(ModernWireError::MissingResultType),
        Some(value) if value.is_string() => Ok(()),
        Some(_) => Err(ModernWireError::MalformedResultType),
    }
}
