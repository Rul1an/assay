//! Mandate Trust Policy (SPEC-Mandate-v1 §6)
//!
//! Configuration for mandate verification and trust rules.
//!
//! # Example Configuration
//!
//! ```yaml
//! mandate_trust:
//!   require_signed: true
//!   expected_audience: "myorg/myapp"
//!   trusted_issuers:
//!     - "auth.myorg.com"
//!   trusted_key_ids:
//!     - "sha256:abc123..."
//!   clock_skew_tolerance_seconds: 30
//!   commit_tools:
//!     - "purchase_*"
//!     - "transfer_*"
//! ```

use crate::mandate::glob::GlobSet;
use serde::{Deserialize, Serialize};

/// Mandate trust policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MandateTrustPolicy {
    /// Require all mandates to be signed
    #[serde(default)]
    pub require_signed: bool,

    /// Expected audience (must match mandate.context.audience)
    /// Format: {org}/{app} or {org}/{app}/{env}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_audience: Option<String>,

    /// Trusted issuers (mandate.context.issuer must be in list)
    /// Comparison is exact string match
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_issuers: Vec<String>,

    /// Trusted signing key IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_key_ids: Vec<String>,

    /// Allow embedded public key (development only)
    #[serde(default)]
    pub allow_embedded_key: bool,

    /// Clock skew tolerance in seconds (default: 30)
    #[serde(default = "default_clock_skew")]
    pub clock_skew_tolerance_seconds: i64,

    /// Trusted sources for lifecycle events (used, revoked)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_event_sources: Vec<String>,

    /// Require signed lifecycle events (recommended for high-risk)
    #[serde(default)]
    pub require_signed_lifecycle_events: bool,

    /// Tool patterns classified as commit operations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_tools: Vec<String>,

    /// Tool patterns classified as write operations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_tools: Vec<String>,
}

fn default_clock_skew() -> i64 {
    30
}

impl MandateTrustPolicy {
    /// Create a new empty policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a permissive development policy.
    pub fn development() -> Self {
        Self {
            require_signed: false,
            allow_embedded_key: true,
            clock_skew_tolerance_seconds: 300, // 5 minutes for dev
            ..Default::default()
        }
    }

    /// Create a strict production policy.
    pub fn production(audience: impl Into<String>) -> Self {
        Self {
            require_signed: true,
            expected_audience: Some(audience.into()),
            allow_embedded_key: false,
            clock_skew_tolerance_seconds: 30,
            require_signed_lifecycle_events: true,
            ..Default::default()
        }
    }

    /// Set expected audience.
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.expected_audience = Some(audience.into());
        self
    }

    /// Add a trusted issuer.
    pub fn with_trusted_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.trusted_issuers.push(issuer.into());
        self
    }

    /// Add a trusted key ID.
    pub fn with_trusted_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.trusted_key_ids.push(key_id.into());
        self
    }

    /// Add commit tool patterns.
    pub fn with_commit_tools(mut self, patterns: Vec<String>) -> Self {
        self.commit_tools = patterns;
        self
    }

    /// Add write tool patterns.
    pub fn with_write_tools(mut self, patterns: Vec<String>) -> Self {
        self.write_tools = patterns;
        self
    }

    /// Check if an issuer is trusted.
    pub fn is_issuer_trusted(&self, issuer: &str) -> bool {
        if self.trusted_issuers.is_empty() {
            // No issuer restrictions
            true
        } else {
            self.trusted_issuers.iter().any(|i| i == issuer)
        }
    }

    /// Check if a key ID is trusted.
    pub fn is_key_trusted(&self, key_id: &str) -> bool {
        if self.trusted_key_ids.is_empty() {
            // No key restrictions (allow any key)
            true
        } else {
            self.trusted_key_ids.iter().any(|k| k == key_id)
        }
    }

    /// Check if audience matches.
    pub fn check_audience(&self, audience: &str) -> bool {
        match &self.expected_audience {
            Some(expected) => expected == audience,
            None => true, // No audience restriction
        }
    }

    /// Check if an event source is trusted for lifecycle events.
    pub fn is_event_source_trusted(&self, source: &str) -> bool {
        if self.trusted_event_sources.is_empty() {
            true
        } else {
            self.trusted_event_sources.iter().any(|s| s == source)
        }
    }

    /// Compile commit tools into a GlobSet for efficient matching.
    pub fn compile_commit_tools(&self) -> Result<GlobSet, crate::mandate::glob::GlobError> {
        GlobSet::new(&self.commit_tools)
    }

    /// Compile write tools into a GlobSet for efficient matching.
    pub fn compile_write_tools(&self) -> Result<GlobSet, crate::mandate::glob::GlobError> {
        GlobSet::new(&self.write_tools)
    }

    /// Determine operation class for a tool based on policy.
    pub fn classify_tool(&self, tool_name: &str) -> ToolClassification {
        // Check commit tools first (highest priority)
        if let Ok(commit_set) = self.compile_commit_tools() {
            if commit_set.matches(tool_name) {
                return ToolClassification::Commit;
            }
        }

        // Check write tools
        if let Ok(write_set) = self.compile_write_tools() {
            if write_set.matches(tool_name) {
                return ToolClassification::Write;
            }
        }

        // Default to read
        ToolClassification::Read
    }
}

/// Tool classification result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClassification {
    /// Read-only operation
    Read,
    /// Write operation (non-financial)
    Write,
    /// Commit operation (financial/irreversible)
    Commit,
}

impl ToolClassification {
    /// Convert to OperationClass.
    pub fn to_operation_class(&self) -> crate::mandate::types::OperationClass {
        match self {
            Self::Read => crate::mandate::types::OperationClass::Read,
            Self::Write => crate::mandate::types::OperationClass::Write,
            Self::Commit => crate::mandate::types::OperationClass::Commit,
        }
    }
}

/// Validation result for a mandate against policy.
#[derive(Debug, Clone)]
pub struct PolicyValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<PolicyValidationError>,
    /// Clock skew applied (if any)
    pub skew_applied_seconds: Option<i64>,
}

impl PolicyValidationResult {
    /// Create a passing result.
    pub fn pass() -> Self {
        Self {
            valid: true,
            errors: vec![],
            skew_applied_seconds: None,
        }
    }

    /// Create a failing result.
    pub fn fail(error: PolicyValidationError) -> Self {
        Self {
            valid: false,
            errors: vec![error],
            skew_applied_seconds: None,
        }
    }

    /// Add clock skew info.
    pub fn with_skew(mut self, skew: i64) -> Self {
        self.skew_applied_seconds = Some(skew);
        self
    }
}

/// Policy validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PolicyValidationError {
    #[error("mandate is not signed but require_signed is true")]
    NotSigned,

    #[error("audience mismatch: expected {expected}, got {got}")]
    AudienceMismatch { expected: String, got: String },

    #[error("issuer not trusted: {issuer}")]
    IssuerNotTrusted { issuer: String },

    #[error("key not trusted: {key_id}")]
    KeyNotTrusted { key_id: String },

    #[error("mandate expired")]
    Expired,

    #[error("mandate not yet valid")]
    NotYetValid,

    #[error("operation class mismatch: tool requires {required:?}, mandate allows {allowed:?}")]
    OperationClassMismatch {
        required: ToolClassification,
        allowed: crate::mandate::types::OperationClass,
    },

    #[error("transaction mandate required for commit operation")]
    TransactionRequired,
}

/// Validate a mandate against policy.
pub fn validate_mandate_policy(
    mandate: &crate::mandate::types::Mandate,
    policy: &MandateTrustPolicy,
    now: chrono::DateTime<chrono::Utc>,
) -> PolicyValidationResult {
    let mut errors = vec![];

    // Check signature requirement
    if policy.require_signed && mandate.signature.is_none() {
        errors.push(PolicyValidationError::NotSigned);
    }

    // Check audience
    if let Some(ref expected) = policy.expected_audience {
        if mandate.context.audience != *expected {
            errors.push(PolicyValidationError::AudienceMismatch {
                expected: expected.clone(),
                got: mandate.context.audience.clone(),
            });
        }
    }

    // Check issuer
    if !policy.is_issuer_trusted(&mandate.context.issuer) {
        errors.push(PolicyValidationError::IssuerNotTrusted {
            issuer: mandate.context.issuer.clone(),
        });
    }

    // Check key ID (if signed)
    if let Some(ref sig) = mandate.signature {
        if !policy.is_key_trusted(&sig.key_id) {
            errors.push(PolicyValidationError::KeyNotTrusted {
                key_id: sig.key_id.clone(),
            });
        }
    }

    // Check validity with clock skew
    let skew = policy.clock_skew_tolerance_seconds;
    if !mandate.validity.is_valid_at_with_skew(now, skew) {
        if let Some(exp) = mandate.validity.expires_at {
            if now >= exp {
                errors.push(PolicyValidationError::Expired);
            }
        }
        if let Some(nb) = mandate.validity.not_before {
            if now < nb {
                errors.push(PolicyValidationError::NotYetValid);
            }
        }
    }

    if errors.is_empty() {
        PolicyValidationResult::pass().with_skew(skew)
    } else {
        PolicyValidationResult {
            valid: false,
            errors,
            skew_applied_seconds: Some(skew),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mandate::types::*;
    use chrono::{TimeZone, Utc};

    fn create_test_mandate() -> Mandate {
        Mandate {
            mandate_id: "sha256:test123".to_string(),
            mandate_kind: MandateKind::Intent,
            principal: Principal::new("user-123", AuthMethod::Oidc),
            scope: Scope::new(vec!["search_*".to_string()]),
            validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap())
                .with_not_before(Utc.with_ymd_and_hms(2026, 1, 28, 9, 0, 0).unwrap())
                .with_expires_at(Utc.with_ymd_and_hms(2026, 1, 28, 18, 0, 0).unwrap()),
            constraints: Constraints::default(),
            context: Context::new("myorg/app", "auth.myorg.com"),
            signature: None,
        }
    }

    #[test]
    fn test_policy_development() {
        let policy = MandateTrustPolicy::development();
        assert!(!policy.require_signed);
        assert!(policy.allow_embedded_key);
        assert_eq!(policy.clock_skew_tolerance_seconds, 300);
    }

    #[test]
    fn test_policy_production() {
        let policy = MandateTrustPolicy::production("myorg/app");
        assert!(policy.require_signed);
        assert!(!policy.allow_embedded_key);
        assert_eq!(policy.expected_audience, Some("myorg/app".to_string()));
    }

    #[test]
    fn test_issuer_check() {
        let policy = MandateTrustPolicy::new()
            .with_trusted_issuer("auth.myorg.com")
            .with_trusted_issuer("idp.partner.com");

        assert!(policy.is_issuer_trusted("auth.myorg.com"));
        assert!(policy.is_issuer_trusted("idp.partner.com"));
        assert!(!policy.is_issuer_trusted("evil.com"));
    }

    #[test]
    fn test_key_check() {
        let policy = MandateTrustPolicy::new().with_trusted_key_id("sha256:abc123");

        assert!(policy.is_key_trusted("sha256:abc123"));
        assert!(!policy.is_key_trusted("sha256:xyz789"));
    }

    #[test]
    fn test_tool_classification() {
        let policy = MandateTrustPolicy::new()
            .with_commit_tools(vec!["purchase_*".to_string(), "transfer_*".to_string()])
            .with_write_tools(vec!["update_*".to_string(), "edit_*".to_string()]);

        assert_eq!(
            policy.classify_tool("purchase_item"),
            ToolClassification::Commit
        );
        assert_eq!(
            policy.classify_tool("transfer_funds"),
            ToolClassification::Commit
        );
        assert_eq!(
            policy.classify_tool("update_profile"),
            ToolClassification::Write
        );
        assert_eq!(
            policy.classify_tool("edit_document"),
            ToolClassification::Write
        );
        assert_eq!(
            policy.classify_tool("search_products"),
            ToolClassification::Read
        );
    }

    #[test]
    fn test_validate_mandate_passes() {
        let mandate = create_test_mandate();
        let policy = MandateTrustPolicy::development();
        let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();

        let result = validate_mandate_policy(&mandate, &policy, now);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_mandate_unsigned_fails() {
        let mandate = create_test_mandate();
        let policy =
            MandateTrustPolicy::production("myorg/app").with_trusted_issuer("auth.myorg.com");
        let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();

        let result = validate_mandate_policy(&mandate, &policy, now);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, PolicyValidationError::NotSigned)));
    }

    #[test]
    fn test_validate_mandate_wrong_audience() {
        let mandate = create_test_mandate();
        let policy = MandateTrustPolicy::new().with_audience("other/app");
        let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();

        let result = validate_mandate_policy(&mandate, &policy, now);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, PolicyValidationError::AudienceMismatch { .. })));
    }

    #[test]
    fn test_validate_mandate_untrusted_issuer() {
        let mandate = create_test_mandate();
        let policy = MandateTrustPolicy::new().with_trusted_issuer("other.com");
        let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();

        let result = validate_mandate_policy(&mandate, &policy, now);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, PolicyValidationError::IssuerNotTrusted { .. })));
    }

    #[test]
    fn test_validate_mandate_expired() {
        let mandate = create_test_mandate();
        let policy = MandateTrustPolicy::development();
        let now = Utc.with_ymd_and_hms(2026, 1, 28, 20, 0, 0).unwrap(); // After expires_at

        let result = validate_mandate_policy(&mandate, &policy, now);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, PolicyValidationError::Expired)));
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
mandate_trust:
  require_signed: true
  expected_audience: "myorg/app"
  trusted_issuers:
    - "auth.myorg.com"
  clock_skew_tolerance_seconds: 60
  commit_tools:
    - "purchase_*"
"#;

        #[derive(Deserialize)]
        struct Config {
            mandate_trust: MandateTrustPolicy,
        }

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.mandate_trust.require_signed);
        assert_eq!(
            config.mandate_trust.expected_audience,
            Some("myorg/app".to_string())
        );
        assert_eq!(config.mandate_trust.trusted_issuers, vec!["auth.myorg.com"]);
        assert_eq!(config.mandate_trust.clock_skew_tolerance_seconds, 60);
        assert_eq!(config.mandate_trust.commit_tools, vec!["purchase_*"]);
    }
}
