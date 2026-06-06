use serde::{Deserialize, Serialize};

/// Context - binding context for replay prevention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// Target application/org identifier
    /// Format: {org}/{app} or {org}/{app}/{env}
    pub audience: String,

    /// Signing authority identifier
    pub issuer: String,

    /// Session binding (for interactive flows)
    /// Minimum 128 bits entropy for transaction mandates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    /// W3C Trace Context for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
}

impl Context {
    /// Create context with required fields.
    pub fn new(audience: impl Into<String>, issuer: impl Into<String>) -> Self {
        Self {
            audience: audience.into(),
            issuer: issuer.into(),
            nonce: None,
            traceparent: None,
        }
    }

    /// Set nonce.
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    /// Set traceparent.
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }
}
