#![allow(deprecated)]

use crate::auth::config::AuthConfig;
use std::env;

fn configured_server_auth_variables() -> Vec<String> {
    let mut names: Vec<String> = env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .filter(|name| name.to_ascii_uppercase().starts_with("ASSAY_AUTH_"))
        .collect();
    names.sort();
    names
}

/// Reject legacy auth configuration for stdio server and proxy modes.
///
/// Only environment-variable names are reported. Values are never read into diagnostics.
pub fn reject_unsupported_stdio_auth_env() -> anyhow::Result<()> {
    let auth_variables = configured_server_auth_variables();
    if !auth_variables.is_empty() {
        anyhow::bail!(
            "ASSAY_AUTH_* configuration is unsupported for stdio server modes; unset: {}",
            auth_variables.join(", ")
        );
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub timeout_ms: u64,
    pub max_msg_bytes: usize,
    pub max_tool_calls: usize,
    pub max_field_bytes: usize,
    pub cache_entries: u64,
    pub log_level: String,
    /// Compatibility-only configuration surface.
    ///
    /// The stdio server does not consume this field for authentication or identity. Server and
    /// proxy binaries reject `ASSAY_AUTH_*` configuration before protocol I/O.
    #[deprecated(
        note = "stdio authentication is unsupported; this legacy field is compatibility-only"
    )]
    pub auth: AuthConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 2000,
            max_msg_bytes: 1_000_000,
            max_tool_calls: 2000,
            max_field_bytes: 64_000,
            cache_entries: 128,
            log_level: "info".to_string(),
            auth: AuthConfig::default(),
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = env::var("ASSAY_MCP_TIMEOUT_MS") {
            if let Ok(n) = v.parse() {
                cfg.timeout_ms = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_msg_bytes = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_FIELD_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_field_bytes = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_MAX_TOOL_CALLS") {
            if let Ok(n) = v.parse() {
                cfg.max_tool_calls = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_MCP_CACHE_ENTRIES") {
            if let Ok(n) = v.parse() {
                cfg.cache_entries = n;
            }
        }
        if let Ok(v) = env::var("ASSAY_LOG") {
            cfg.log_level = v;
        }
        cfg
    }
}
