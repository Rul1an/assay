//! Exact denial-marker triples for privileged-mcp-action v0 and v1.
//!
//! One function answers "is this payload a caller-visible proxy-denial marker?".
//! Match is the whole triple (schema, integer code, origin). Code-only, origin-only,
//! string codes, missing members, and cross-version pairs stay inert.

use serde_json::Value;

pub const DENIED_CALL_OBSERVATION_V0: &str = "assay.denied_call_observation.v0";
pub const DENIED_CALL_OBSERVATION_V1: &str = "assay.denied_call_observation.v1";
pub const PROXY_ORIGIN: &str = "assay-proxy";
pub const PROXY_DENIED_V0: i64 = -32042;
/// Application-defined deny code outside JSON-RPC `-32768..=-32000` and MCP's
/// legacy/reserved partitions. #2509 will emit this; #2508 only recognizes it.
pub const PROXY_DENIED_V1: i64 = -31999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialMarkerVersion {
    V0,
    V1,
}

/// Returns `Some` only for an exact shipped triple. Never matches by code or origin alone.
pub fn classify_denial_marker(payload: &Value) -> Option<DenialMarkerVersion> {
    let schema = payload.get("schema").and_then(Value::as_str)?;
    let code = payload
        .pointer("/caller_visible_error/code")
        .and_then(Value::as_i64)?;
    let origin = payload
        .pointer("/caller_visible_error/origin")
        .and_then(Value::as_str)?;
    match (schema, code, origin) {
        (DENIED_CALL_OBSERVATION_V0, PROXY_DENIED_V0, PROXY_ORIGIN) => {
            Some(DenialMarkerVersion::V0)
        }
        (DENIED_CALL_OBSERVATION_V1, PROXY_DENIED_V1, PROXY_ORIGIN) => {
            Some(DenialMarkerVersion::V1)
        }
        _ => None,
    }
}

/// Bindable marker: classified triple plus non-empty tool name and target digest.
/// A classified marker with a null or empty digest is unbindable and out of scope.
pub fn bindable_denial_marker(payload: &Value) -> Option<BindableDenialMarker<'_>> {
    let version = classify_denial_marker(payload)?;
    let tool_name = non_empty(payload.pointer("/call/tool_name")?.as_str()?)?;
    let target_digest = non_empty(payload.pointer("/call/target_digest")?.as_str()?)?;
    Some(BindableDenialMarker {
        version,
        tool_name,
        target_digest,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindableDenialMarker<'a> {
    pub version: DenialMarkerVersion,
    pub tool_name: &'a str,
    pub target_digest: &'a str,
}

fn non_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
