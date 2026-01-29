//! Error types for the registry client.

use std::time::Duration;

/// Registry errors.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Pack not found in registry.
    #[error("pack not found: {name}@{version}")]
    NotFound { name: String, version: String },

    /// Authentication failed or token invalid.
    #[error("unauthorized: {message}")]
    Unauthorized { message: String },

    /// Rate limit exceeded.
    #[error("rate limited: retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },

    /// Pack has been revoked (410 Gone).
    #[error("pack revoked: {name}@{version} - {reason}")]
    Revoked {
        name: String,
        version: String,
        reason: String,
    },

    /// Digest verification failed.
    #[error("digest mismatch for {name}@{version}: expected {expected}, got {actual}")]
    DigestMismatch {
        name: String,
        version: String,
        expected: String,
        actual: String,
    },

    /// Signature verification failed.
    #[error("signature verification failed: {reason}")]
    SignatureInvalid { reason: String },

    /// Key not trusted.
    #[error("key not trusted: {key_id}")]
    KeyNotTrusted { key_id: String },

    /// Pack is unsigned but unsigned packs are not allowed.
    #[error("pack is unsigned: {name}@{version}")]
    Unsigned { name: String, version: String },

    /// Invalid pack reference format.
    #[error("invalid pack reference: {reference} - {reason}")]
    InvalidReference { reference: String, reason: String },

    /// Network error.
    #[error("network error: {message}")]
    Network { message: String },

    /// Cache error.
    #[error("cache error: {message}")]
    Cache { message: String },

    /// Invalid response from registry.
    #[error("invalid response: {message}")]
    InvalidResponse { message: String },

    /// Configuration error.
    #[error("configuration error: {message}")]
    Config { message: String },

    /// Lockfile error.
    #[error("lockfile error: {message}")]
    Lockfile { message: String },
}

impl RegistryError {
    /// Exit code for CLI.
    pub fn exit_code(&self) -> i32 {
        match self {
            // Not found / config issues
            Self::NotFound { .. } => 1,
            Self::Config { .. } => 1,
            Self::InvalidReference { .. } => 1,

            // Auth issues
            Self::Unauthorized { .. } => 2,

            // Security issues (higher priority)
            Self::Revoked { .. } => 3,
            Self::DigestMismatch { .. } => 4,
            Self::SignatureInvalid { .. } => 4,
            Self::KeyNotTrusted { .. } => 4,
            Self::Unsigned { .. } => 4,

            // Network/transient
            Self::RateLimited { .. } => 5,
            Self::Network { .. } => 5,

            // Other
            Self::Cache { .. } => 6,
            Self::InvalidResponse { .. } => 6,
            Self::Lockfile { .. } => 7,
        }
    }

    /// Whether the error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Network { .. })
    }
}

impl From<reqwest::Error> for RegistryError {
    fn from(err: reqwest::Error) -> Self {
        Self::Network {
            message: err.to_string(),
        }
    }
}

/// Result type for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;
