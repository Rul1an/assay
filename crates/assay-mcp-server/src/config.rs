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

/// Inclusive local-policy ingest ceiling. Invalid or absent
/// `ASSAY_MCP_MAX_POLICY_BYTES` keeps this default. Independent of
/// `ASSAY_MCP_MAX_BYTES`.
pub(crate) const DEFAULT_POLICY_BYTE_LIMIT: usize = 1_000_000;

/// Resolve the policy-file byte ceiling from the process environment.
///
/// One parser for startup logging and every production policy read. Not stored
/// on `ServerConfig`: that public struct must keep the v5.2.0 field set.
/// Visible to the stdio binary (`src/main.rs`), which is a separate crate.
pub fn policy_byte_limit_from_env() -> usize {
    match env::var("ASSAY_MCP_MAX_POLICY_BYTES") {
        Ok(value) => value.parse().unwrap_or(DEFAULT_POLICY_BYTE_LIMIT),
        Err(_) => DEFAULT_POLICY_BYTE_LIMIT,
    }
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

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        _guard: MutexGuard<'static, ()>,
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvRestore {
        fn apply(pairs: &[(&'static str, Option<&str>)]) -> Self {
            let guard = ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let saved = pairs
                .iter()
                .map(|(name, _)| (*name, env::var(name).ok()))
                .collect();
            for (name, value) in pairs {
                match value {
                    Some(value) => unsafe { env::set_var(name, value) },
                    None => unsafe { env::remove_var(name) },
                }
            }
            Self {
                _guard: guard,
                saved,
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                match value {
                    Some(value) => unsafe { env::set_var(name, value) },
                    None => unsafe { env::remove_var(name) },
                }
            }
        }
    }

    #[test]
    fn default_policy_ceiling_is_one_million() {
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", None),
            ("ASSAY_MCP_MAX_BYTES", None),
        ]);
        assert_eq!(policy_byte_limit_from_env(), DEFAULT_POLICY_BYTE_LIMIT);
        assert_eq!(ServerConfig::from_env().max_msg_bytes, 1_000_000);
    }

    #[test]
    fn policy_and_message_ceilings_diverge_in_both_directions() {
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", Some("1234")),
            ("ASSAY_MCP_MAX_BYTES", Some("4321")),
        ]);
        assert_eq!(policy_byte_limit_from_env(), 1234);
        assert_eq!(ServerConfig::from_env().max_msg_bytes, 4321);

        drop(_env);
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", Some("4321")),
            ("ASSAY_MCP_MAX_BYTES", Some("1234")),
        ]);
        assert_eq!(
            policy_byte_limit_from_env(),
            4321,
            "policy override must not follow ASSAY_MCP_MAX_BYTES"
        );
        assert_eq!(
            ServerConfig::from_env().max_msg_bytes,
            1234,
            "message override must not follow ASSAY_MCP_MAX_POLICY_BYTES"
        );
    }

    #[test]
    fn each_ceiling_override_leaves_the_other_at_default() {
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", Some("1234")),
            ("ASSAY_MCP_MAX_BYTES", None),
        ]);
        assert_eq!(policy_byte_limit_from_env(), 1234);
        assert_eq!(ServerConfig::from_env().max_msg_bytes, 1_000_000);

        drop(_env);
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", None),
            ("ASSAY_MCP_MAX_BYTES", Some("4321")),
        ]);
        assert_eq!(policy_byte_limit_from_env(), DEFAULT_POLICY_BYTE_LIMIT);
        assert_eq!(ServerConfig::from_env().max_msg_bytes, 4321);
    }

    #[test]
    fn invalid_policy_override_keeps_default_and_does_not_touch_message_ceiling() {
        let _env = EnvRestore::apply(&[
            ("ASSAY_MCP_MAX_POLICY_BYTES", Some("not-a-number")),
            ("ASSAY_MCP_MAX_BYTES", Some("4321")),
        ]);
        assert_eq!(policy_byte_limit_from_env(), DEFAULT_POLICY_BYTE_LIMIT);
        assert_eq!(ServerConfig::from_env().max_msg_bytes, 4321);
    }
}
