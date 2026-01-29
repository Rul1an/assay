//! Runtime mandate authorization.
//!
//! Implements SPEC-Mandate-v1.0.3 §7: Runtime Enforcement.
//!
//! Flow:
//! 1. Verify validity window (§7.6)
//! 2. Verify scope matches tool
//! 3. Verify mandate_kind matches operation_class
//! 4. Verify transaction_ref for commit tools (§7.7)
//! 5. Consume mandate atomically (§7.4)

use super::mandate_store::{
    AuthzError, AuthzReceipt, ConsumeParams, MandateMetadata, MandateStore,
};
use chrono::{DateTime, Duration, Utc};
use thiserror::Error;

/// Default clock skew tolerance in seconds.
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 30;

/// Authorization configuration.
#[derive(Debug, Clone)]
pub struct AuthzConfig {
    /// Clock skew tolerance for validity checks.
    pub clock_skew_seconds: i64,
    /// Expected audience (must match mandate.context.audience).
    pub expected_audience: String,
    /// Trusted issuers (mandate.context.issuer must be in this list).
    pub trusted_issuers: Vec<String>,
}

impl Default for AuthzConfig {
    fn default() -> Self {
        Self {
            clock_skew_seconds: DEFAULT_CLOCK_SKEW_SECONDS,
            expected_audience: String::new(),
            trusted_issuers: Vec::new(),
        }
    }
}

/// Operation class for tool classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationClass {
    Read = 0,
    Write = 1,
    Commit = 2,
}

impl OperationClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Commit => "commit",
        }
    }
}

/// Mandate kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MandateKind {
    Intent,
    Transaction,
}

impl MandateKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Transaction => "transaction",
        }
    }

    /// Returns the maximum operation class this mandate kind allows.
    pub fn max_operation_class(&self) -> OperationClass {
        match self {
            Self::Intent => OperationClass::Write, // intent allows read, write
            Self::Transaction => OperationClass::Commit, // transaction allows all
        }
    }
}

/// Mandate data for authorization (extracted from signed mandate).
#[derive(Debug, Clone)]
pub struct MandateData {
    pub mandate_id: String,
    pub mandate_kind: MandateKind,
    pub audience: String,
    pub issuer: String,
    pub tool_patterns: Vec<String>,
    pub operation_class: Option<OperationClass>,
    pub transaction_ref: Option<String>,
    pub not_before: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub single_use: bool,
    pub max_uses: Option<u32>,
    pub nonce: Option<String>,
    pub canonical_digest: String,
    pub key_id: String,
}

/// Tool call data for authorization.
#[derive(Debug, Clone)]
pub struct ToolCallData {
    pub tool_call_id: String,
    pub tool_name: String,
    pub operation_class: OperationClass,
    pub transaction_object: Option<serde_json::Value>,
    pub source_run_id: Option<String>,
}

/// Policy-level authorization errors (before DB).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("Mandate expired: expires_at={expires_at}, now={now}")]
    Expired {
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },

    #[error("Mandate not yet valid: not_before={not_before}, now={now}")]
    NotYetValid {
        not_before: DateTime<Utc>,
        now: DateTime<Utc>,
    },

    #[error("Tool '{tool}' not in mandate scope")]
    ToolNotInScope { tool: String },

    #[error("Mandate kind '{kind}' does not allow operation class '{op_class}'")]
    KindMismatch { kind: String, op_class: String },

    #[error("Audience mismatch: expected '{expected}', got '{actual}'")]
    AudienceMismatch { expected: String, actual: String },

    #[error("Issuer '{issuer}' not in trusted issuers")]
    IssuerNotTrusted { issuer: String },

    #[error("Missing transaction object for commit tool")]
    MissingTransactionObject,

    #[error("Transaction ref mismatch: expected '{expected}', got '{actual}'")]
    TransactionRefMismatch { expected: String, actual: String },
}

/// Combined authorization error.
#[derive(Debug, Error)]
pub enum AuthorizeError {
    #[error("Policy error: {0}")]
    Policy(#[from] PolicyError),

    #[error("Store error: {0}")]
    Store(#[from] AuthzError),

    #[error("Failed to compute transaction ref: {0}")]
    TransactionRef(String),
}

/// Runtime authorizer.
pub struct Authorizer {
    store: MandateStore,
    config: AuthzConfig,
}

impl Authorizer {
    /// Create a new authorizer with the given store and config.
    pub fn new(store: MandateStore, config: AuthzConfig) -> Self {
        Self { store, config }
    }

    /// Authorize and consume a mandate for a tool call.
    ///
    /// Implements SPEC-Mandate-v1.0.3 §7 flow:
    /// 1. Verify validity window
    /// 2. Verify context (audience, issuer)
    /// 3. Verify scope matches tool
    /// 4. Verify mandate_kind matches operation_class
    /// 5. Verify transaction_ref for commit tools
    /// 6. Upsert mandate metadata
    /// 7. Consume mandate atomically
    pub fn authorize_and_consume(
        &self,
        mandate: &MandateData,
        tool_call: &ToolCallData,
    ) -> Result<AuthzReceipt, AuthorizeError> {
        let now = Utc::now();
        let skew = Duration::seconds(self.config.clock_skew_seconds);

        // 1. Verify validity window (§7.6)
        if let Some(not_before) = mandate.not_before {
            if now < not_before - skew {
                return Err(PolicyError::NotYetValid { not_before, now }.into());
            }
        }
        if let Some(expires_at) = mandate.expires_at {
            if now >= expires_at + skew {
                return Err(PolicyError::Expired { expires_at, now }.into());
            }
        }

        // 1b. Check revocation status (P0-A)
        if let Some(revoked_at) = self.store.get_revoked_at(&mandate.mandate_id)? {
            if now >= revoked_at {
                return Err(AuthzError::Revoked { revoked_at }.into());
            }
        }

        // 2. Verify context
        if !self.config.expected_audience.is_empty()
            && mandate.audience != self.config.expected_audience
        {
            return Err(PolicyError::AudienceMismatch {
                expected: self.config.expected_audience.clone(),
                actual: mandate.audience.clone(),
            }
            .into());
        }
        if !self.config.trusted_issuers.is_empty()
            && !self.config.trusted_issuers.contains(&mandate.issuer)
        {
            return Err(PolicyError::IssuerNotTrusted {
                issuer: mandate.issuer.clone(),
            }
            .into());
        }

        // 3. Verify scope matches tool
        if !self.tool_matches_scope(&tool_call.tool_name, &mandate.tool_patterns) {
            return Err(PolicyError::ToolNotInScope {
                tool: tool_call.tool_name.clone(),
            }
            .into());
        }

        // 4. Verify mandate_kind matches operation_class
        let max_allowed = mandate.mandate_kind.max_operation_class();
        if tool_call.operation_class > max_allowed {
            return Err(PolicyError::KindMismatch {
                kind: mandate.mandate_kind.as_str().to_string(),
                op_class: tool_call.operation_class.as_str().to_string(),
            }
            .into());
        }

        // 5. Verify transaction_ref for commit tools (§7.7)
        if tool_call.operation_class == OperationClass::Commit {
            if let Some(expected_ref) = &mandate.transaction_ref {
                let tx_obj = tool_call
                    .transaction_object
                    .as_ref()
                    .ok_or(PolicyError::MissingTransactionObject)?;

                let actual_ref = compute_transaction_ref(tx_obj)
                    .map_err(|e| AuthorizeError::TransactionRef(e.to_string()))?;

                if actual_ref != *expected_ref {
                    return Err(PolicyError::TransactionRefMismatch {
                        expected: expected_ref.clone(),
                        actual: actual_ref,
                    }
                    .into());
                }
            }
        }

        // 6. Upsert mandate metadata
        let meta = MandateMetadata {
            mandate_id: mandate.mandate_id.clone(),
            mandate_kind: mandate.mandate_kind.as_str().to_string(),
            audience: mandate.audience.clone(),
            issuer: mandate.issuer.clone(),
            expires_at: mandate.expires_at,
            single_use: mandate.single_use,
            max_uses: mandate.max_uses,
            canonical_digest: mandate.canonical_digest.clone(),
            key_id: mandate.key_id.clone(),
        };
        self.store.upsert_mandate(&meta)?;

        // 7. Consume mandate atomically
        let receipt = self.store.consume_mandate(&ConsumeParams {
            mandate_id: &mandate.mandate_id,
            tool_call_id: &tool_call.tool_call_id,
            nonce: mandate.nonce.as_deref(),
            audience: &mandate.audience,
            issuer: &mandate.issuer,
            tool_name: &tool_call.tool_name,
            operation_class: tool_call.operation_class.as_str(),
            source_run_id: tool_call.source_run_id.as_deref(),
        })?;

        Ok(receipt)
    }

    /// Check if tool name matches any of the scope patterns.
    fn tool_matches_scope(&self, tool_name: &str, patterns: &[String]) -> bool {
        for pattern in patterns {
            if glob_matches(pattern, tool_name) {
                return true;
            }
        }
        false
    }
}

/// Simple glob matching for tool patterns.
///
/// Supports:
/// - `*` matches any characters except `.`
/// - `**` matches any characters including `.`
/// - Literal characters match exactly
fn glob_matches(pattern: &str, input: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut input_chars = input.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Check for **
                if pattern_chars.peek() == Some(&'*') {
                    pattern_chars.next(); // consume second *
                                          // ** matches everything including dots
                    let remaining: String = pattern_chars.collect();
                    if remaining.is_empty() {
                        return true; // ** at end matches everything
                    }
                    // Try matching remaining pattern at every position
                    let remaining_input: String = input_chars.collect();
                    for i in 0..=remaining_input.len() {
                        if glob_matches(&remaining, &remaining_input[i..]) {
                            return true;
                        }
                    }
                    return false;
                } else {
                    // * matches everything except dot
                    let remaining: String = pattern_chars.collect();
                    if remaining.is_empty() {
                        // * at end - consume until dot or end
                        return input_chars.all(|c| c != '.');
                    }
                    // Try matching remaining pattern at every position (stopping at dot)
                    let mut remaining_input: String = input_chars.collect();
                    loop {
                        if glob_matches(&remaining, &remaining_input) {
                            return true;
                        }
                        if remaining_input.is_empty() || remaining_input.starts_with('.') {
                            return false;
                        }
                        remaining_input = remaining_input[1..].to_string();
                    }
                }
            }
            '\\' => {
                // Escape sequence
                if let Some(escaped) = pattern_chars.next() {
                    if input_chars.next() != Some(escaped) {
                        return false;
                    }
                } else {
                    return false; // Trailing backslash
                }
            }
            c => {
                if input_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    // Pattern consumed, input should also be consumed
    input_chars.next().is_none()
}

/// Compute transaction_ref hash from transaction object.
fn compute_transaction_ref(tx_object: &serde_json::Value) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    // Canonicalize using JCS (RFC 8785)
    let canonical = serde_jcs::to_vec(tx_object).map_err(|e| e.to_string())?;

    let hash = Sha256::digest(&canonical);
    Ok(format!("sha256:{}", hex::encode(hash)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AuthzConfig {
        AuthzConfig {
            clock_skew_seconds: 30,
            expected_audience: "org/app".to_string(),
            trusted_issuers: vec!["auth.org.com".to_string()],
        }
    }

    fn test_mandate() -> MandateData {
        MandateData {
            mandate_id: "sha256:test123".to_string(),
            mandate_kind: MandateKind::Intent,
            audience: "org/app".to_string(),
            issuer: "auth.org.com".to_string(),
            tool_patterns: vec!["search_*".to_string(), "get_*".to_string()],
            operation_class: Some(OperationClass::Read),
            transaction_ref: None,
            not_before: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            single_use: false,
            max_uses: None,
            nonce: None,
            canonical_digest: "sha256:digest123".to_string(),
            key_id: "sha256:key123".to_string(),
        }
    }

    fn test_tool_call(name: &str) -> ToolCallData {
        ToolCallData {
            tool_call_id: format!("tc_{}", name),
            tool_name: name.to_string(),
            operation_class: OperationClass::Read,
            transaction_object: None,
            source_run_id: None,
        }
    }

    // === Glob matching tests ===

    #[test]
    fn test_glob_exact_match() {
        assert!(glob_matches("search", "search"));
        assert!(!glob_matches("search", "search_products"));
        assert!(!glob_matches("search", "my_search"));
    }

    #[test]
    fn test_glob_single_star() {
        assert!(glob_matches("search_*", "search_products"));
        assert!(glob_matches("search_*", "search_users"));
        assert!(glob_matches("search_*", "search_"));
        assert!(!glob_matches("search_*", "search.products")); // * stops at dot
    }

    #[test]
    fn test_glob_double_star() {
        assert!(glob_matches("fs.**", "fs.read_file"));
        assert!(glob_matches("fs.**", "fs.write.nested.path"));
        assert!(glob_matches("**", "anything.at.all"));
    }

    #[test]
    fn test_glob_escaped() {
        assert!(glob_matches(r"file\*name", "file*name"));
        assert!(!glob_matches(r"file\*name", "filename"));
    }

    // === Validity window tests (§7.6) ===

    #[test]
    fn test_authorize_rejects_expired() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.expires_at = Some(Utc::now() - Duration::seconds(31)); // Beyond skew

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::Expired { .. }))
        ));
    }

    #[test]
    fn test_authorize_allows_within_expiry_skew() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.expires_at = Some(Utc::now() - Duration::seconds(5)); // Within skew

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(result.is_ok());
    }

    #[test]
    fn test_authorize_rejects_not_yet_valid() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.not_before = Some(Utc::now() + Duration::seconds(31)); // Beyond skew

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::NotYetValid { .. }))
        ));
    }

    // === Scope tests ===

    #[test]
    fn test_authorize_rejects_tool_not_in_scope() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mandate = test_mandate(); // scope: search_*, get_*
        let tool_call = test_tool_call("purchase_item"); // Not in scope

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::ToolNotInScope { tool })) if tool == "purchase_item"
        ));
    }

    #[test]
    fn test_authorize_allows_tool_in_scope() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mandate = test_mandate();
        let tool_call = test_tool_call("search_products");

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);
        assert!(result.is_ok());
    }

    // === Kind/operation_class tests ===

    #[test]
    fn test_authorize_rejects_commit_with_intent_mandate() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.mandate_kind = MandateKind::Intent;
        mandate.tool_patterns = vec!["purchase_*".to_string()];

        let mut tool_call = test_tool_call("purchase_item");
        tool_call.operation_class = OperationClass::Commit;

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::KindMismatch { .. }))
        ));
    }

    #[test]
    fn test_authorize_allows_commit_with_transaction_mandate() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.mandate_kind = MandateKind::Transaction;
        mandate.tool_patterns = vec!["purchase_*".to_string()];

        let mut tool_call = test_tool_call("purchase_item");
        tool_call.operation_class = OperationClass::Commit;

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);
        assert!(result.is_ok());
    }

    // === transaction_ref tests (§7.7) ===

    #[test]
    fn test_authorize_rejects_missing_transaction_object() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.mandate_kind = MandateKind::Transaction;
        mandate.tool_patterns = vec!["purchase_*".to_string()];
        mandate.transaction_ref = Some("sha256:expected".to_string());

        let mut tool_call = test_tool_call("purchase_item");
        tool_call.operation_class = OperationClass::Commit;
        tool_call.transaction_object = None; // Missing!

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(
                PolicyError::MissingTransactionObject
            ))
        ));
    }

    #[test]
    fn test_authorize_rejects_transaction_ref_mismatch() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        // Compute expected ref from a specific object
        let expected_obj = serde_json::json!({
            "merchant_id": "shop_123",
            "amount_cents": 4999,
            "currency": "EUR"
        });
        let expected_ref = compute_transaction_ref(&expected_obj).unwrap();

        let mut mandate = test_mandate();
        mandate.mandate_kind = MandateKind::Transaction;
        mandate.tool_patterns = vec!["purchase_*".to_string()];
        mandate.transaction_ref = Some(expected_ref);

        let mut tool_call = test_tool_call("purchase_item");
        tool_call.operation_class = OperationClass::Commit;
        // Different transaction object!
        tool_call.transaction_object = Some(serde_json::json!({
            "merchant_id": "shop_123",
            "amount_cents": 9999, // Different amount
            "currency": "EUR"
        }));

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(
                PolicyError::TransactionRefMismatch { .. }
            ))
        ));
    }

    #[test]
    fn test_authorize_allows_matching_transaction_ref() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store, config);

        let tx_obj = serde_json::json!({
            "merchant_id": "shop_123",
            "amount_cents": 4999,
            "currency": "EUR"
        });
        let tx_ref = compute_transaction_ref(&tx_obj).unwrap();

        let mut mandate = test_mandate();
        mandate.mandate_kind = MandateKind::Transaction;
        mandate.tool_patterns = vec!["purchase_*".to_string()];
        mandate.transaction_ref = Some(tx_ref);

        let mut tool_call = test_tool_call("purchase_item");
        tool_call.operation_class = OperationClass::Commit;
        tool_call.transaction_object = Some(tx_obj);

        let result = authorizer.authorize_and_consume(&mandate, &tool_call);
        assert!(result.is_ok());
    }

    // === Context tests ===

    #[test]
    fn test_authorize_rejects_wrong_audience() {
        let store = MandateStore::memory().unwrap();
        let config = test_config(); // expects "org/app"
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.audience = "other/app".to_string();

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::AudienceMismatch { .. }))
        ));
    }

    #[test]
    fn test_authorize_rejects_untrusted_issuer() {
        let store = MandateStore::memory().unwrap();
        let config = test_config(); // trusts "auth.org.com"
        let authorizer = Authorizer::new(store, config);

        let mut mandate = test_mandate();
        mandate.issuer = "evil.attacker.com".to_string();

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(matches!(
            result,
            Err(AuthorizeError::Policy(PolicyError::IssuerNotTrusted { .. }))
        ));
    }

    // === Revocation tests (P0-A) ===

    #[test]
    fn test_authorize_rejects_revoked_mandate() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store.clone(), config);

        let mandate = test_mandate();

        // Revoke it before first use
        store
            .upsert_revocation(&super::super::mandate_store::RevocationRecord {
                mandate_id: mandate.mandate_id.clone(),
                revoked_at: Utc::now() - chrono::Duration::minutes(5),
                reason: Some("User requested".to_string()),
                revoked_by: None,
                source: None,
                event_id: None,
            })
            .unwrap();

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(
            matches!(
                result,
                Err(AuthorizeError::Store(AuthzError::Revoked { .. }))
            ),
            "Expected Revoked error, got {:?}",
            result
        );
    }

    #[test]
    fn test_authorize_allows_if_revoked_in_future() {
        let store = MandateStore::memory().unwrap();
        let config = test_config();
        let authorizer = Authorizer::new(store.clone(), config);

        let mandate = test_mandate();

        // Revocation is in the future (shouldn't block yet)
        store
            .upsert_revocation(&super::super::mandate_store::RevocationRecord {
                mandate_id: mandate.mandate_id.clone(),
                revoked_at: Utc::now() + chrono::Duration::hours(1),
                reason: Some("Scheduled revocation".to_string()),
                revoked_by: None,
                source: None,
                event_id: None,
            })
            .unwrap();

        let tool_call = test_tool_call("search_products");
        let result = authorizer.authorize_and_consume(&mandate, &tool_call);

        assert!(result.is_ok(), "Should allow use before revoked_at");
    }
}
