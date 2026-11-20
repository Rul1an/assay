// on_error.rs - Fail-safe configuration for Assay
//
// This module defines the error handling policy for policy checks.
// See docs/concepts/fail-safe.md for usage documentation.

#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;

assay_common::closed! {
    /// Error handling policy for checks such as schema parsing and network evaluation.
    /// `Block` (default) denies on evaluation error. `Allow` permits the action and emits a
    /// warning when availability is more important than failing closed.
    #[derive(Copy, Eq, Default, Serialize)]
    pub enum ErrorPolicy {
        #[default]
        "block" => Block,
        "allow" => Allow,
    }
}

impl ErrorPolicy {
    /// Returns true if this policy blocks on error
    pub fn blocks_on_error(&self) -> bool {
        matches!(self, ErrorPolicy::Block)
    }

    /// Returns true if this policy allows on error
    pub fn allows_on_error(&self) -> bool {
        matches!(self, ErrorPolicy::Allow)
    }

    /// Apply the policy to an error, returning the appropriate TestStatus
    pub fn apply_to_error(&self, error: &anyhow::Error) -> ErrorPolicyResult {
        match self {
            ErrorPolicy::Block => ErrorPolicyResult::Blocked {
                reason: format!("Policy check error (fail-closed): {}", error),
            },
            ErrorPolicy::Allow => ErrorPolicyResult::Allowed {
                warning: format!("Policy check error (fail-open): {}", error),
            },
        }
    }
}

/// Result of applying an error policy
#[derive(Debug, Clone)]
pub enum ErrorPolicyResult {
    /// Action was blocked due to error (fail-closed)
    Blocked { reason: String },
    /// Action was allowed despite error (fail-open)
    Allowed { warning: String },
}

/// Logs a fail-safe trigger event in structured JSON format.
///
/// This provides Ops teams with a machine-readable audit trail when the
/// fail-safe mechanism permits execution despite an error.
pub fn log_fail_safe(reason: &str, config_path: Option<&str>) {
    // SOTA: Use structured tracing instead of println
    // This allows the binary (CLI or MCP server) to route logs to OTLP/Datadog
    tracing::warn!(
        event = "assay.failsafe.triggered",
        reason = %reason,
        config_path = %config_path.unwrap_or("none"),
        action = "allowed",
        "Fail-safe triggered: {}", reason
    );
}

impl ErrorPolicyResult {
    pub fn is_blocked(&self) -> bool {
        matches!(self, ErrorPolicyResult::Blocked { .. })
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self, ErrorPolicyResult::Allowed { .. })
    }

    pub fn message(&self) -> &str {
        match self {
            ErrorPolicyResult::Blocked { reason } => reason,
            ErrorPolicyResult::Allowed { warning } => warning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_block() {
        assert_eq!(ErrorPolicy::default(), ErrorPolicy::Block);
    }

    #[test]
    fn test_block_policy() {
        let policy = ErrorPolicy::Block;
        assert!(policy.blocks_on_error());
        assert!(!policy.allows_on_error());

        let error = anyhow::anyhow!("Schema parse failed");
        let result = policy.apply_to_error(&error);
        assert!(result.is_blocked());
    }

    #[test]
    fn test_allow_policy() {
        let policy = ErrorPolicy::Allow;
        assert!(!policy.blocks_on_error());
        assert!(policy.allows_on_error());

        let error = anyhow::anyhow!("Network timeout");
        let result = policy.apply_to_error(&error);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_serde_roundtrip() {
        let block: ErrorPolicy = serde_yaml::from_str("block").unwrap();
        assert_eq!(block, ErrorPolicy::Block);

        let allow: ErrorPolicy = serde_yaml::from_str("allow").unwrap();
        assert_eq!(allow, ErrorPolicy::Allow);

        // Test in struct context
        #[derive(Deserialize)]
        struct Config {
            on_error: ErrorPolicy,
        }

        let config: Config = serde_yaml::from_str("on_error: block").unwrap();
        assert_eq!(config.on_error, ErrorPolicy::Block);

        let config: Config = serde_yaml::from_str("on_error: allow").unwrap();
        assert_eq!(config.on_error, ErrorPolicy::Allow);
    }
}
