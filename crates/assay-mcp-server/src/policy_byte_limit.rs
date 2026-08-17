//! Shared policy-byte ceiling parser for the library crate and the stdio binary.
//!
//! Both crates compile this file as a private module (`#[path]`). The function
//! stays `pub(crate)` so it is never part of the published `assay-mcp-server`
//! public API.

use std::env;

/// Inclusive local-policy ingest ceiling. Invalid or absent
/// `ASSAY_MCP_MAX_POLICY_BYTES` keeps this default. Independent of
/// `ASSAY_MCP_MAX_BYTES`.
pub(crate) const DEFAULT_POLICY_BYTE_LIMIT: usize = 1_000_000;

/// Resolve the policy-file byte ceiling from the process environment.
///
/// One parser for startup logging and every production policy read. Not stored
/// on `ServerConfig`: that public struct must keep the v5.2.0 field set.
pub(crate) fn policy_byte_limit_from_env() -> usize {
    match env::var("ASSAY_MCP_MAX_POLICY_BYTES") {
        Ok(value) => value.parse().unwrap_or(DEFAULT_POLICY_BYTE_LIMIT),
        Err(_) => DEFAULT_POLICY_BYTE_LIMIT,
    }
}
