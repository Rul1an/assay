use serde::{Deserialize, Serialize};

use super::enums::AuthMethod;

/// Principal - who granted the mandate.
///
/// # Privacy Requirements
///
/// - `subject` MUST be opaque; MUST NOT contain email, name, or other PII
/// - `display` is for UX only; verifiers MUST NOT use it for trust decisions
/// - `display` SHOULD be absent in exported audit bundles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Opaque identifier (MUST NOT contain PII)
    pub subject: String,

    /// Authentication method
    pub method: AuthMethod,

    /// Human-readable name (UX only, MUST NOT use for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// Hash reference to verifiable credential
    /// Format: "sha256:" + lowercase_hex(SHA256(credential_bytes))
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
}

impl Principal {
    /// Create a new principal with required fields.
    pub fn new(subject: impl Into<String>, method: AuthMethod) -> Self {
        Self {
            subject: subject.into(),
            method,
            display: None,
            credential_ref: None,
        }
    }

    /// Set display name (UX only).
    pub fn with_display(mut self, display: impl Into<String>) -> Self {
        self.display = Some(display.into());
        self
    }

    /// Set credential reference.
    pub fn with_credential_ref(mut self, credential_ref: impl Into<String>) -> Self {
        self.credential_ref = Some(credential_ref.into());
        self
    }
}
