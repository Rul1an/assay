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

use super::mandate_store::{AuthzError, AuthzReceipt, MandateStore};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[path = "authorizer_internal/mod.rs"]
mod authorizer_internal;

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
        authorizer_internal::run::authorize_and_consume_impl(self, mandate, tool_call)
    }

    /// Like [`authorize_and_consume`] but with an explicit `now` timestamp.
    /// Use this in tests to avoid flaky clock-dependent assertions.
    pub fn authorize_at(
        &self,
        now: DateTime<Utc>,
        mandate: &MandateData,
        tool_call: &ToolCallData,
    ) -> Result<AuthzReceipt, AuthorizeError> {
        authorizer_internal::run::authorize_at_impl(self, now, mandate, tool_call)
    }
}
