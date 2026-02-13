//! Canonicalization error types and limits.

use crate::error::RegistryError;

/// Maximum nesting depth for YAML structures.
pub const MAX_DEPTH: usize = 50;

/// Maximum number of keys in a single mapping.
pub const MAX_KEYS_PER_MAPPING: usize = 10_000;

/// Maximum string length (1MB).
pub const MAX_STRING_LENGTH: usize = 1_024 * 1_024;

/// Maximum total YAML size (10MB).
pub const MAX_TOTAL_SIZE: usize = 10 * 1_024 * 1_024;

/// Maximum safe integer value (2^53 for JSON compatibility).
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_992; // 2^53

/// Minimum safe integer value (-2^53 for JSON compatibility).
pub const MIN_SAFE_INTEGER: i64 = -9_007_199_254_740_992; // -2^53

/// Errors specific to canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalizeError {
    /// YAML contains anchors (forbidden).
    AnchorFound { position: String },

    /// YAML contains aliases (forbidden).
    AliasFound { position: String },

    /// YAML contains tags (forbidden).
    TagFound { tag: String },

    /// YAML contains multiple documents (forbidden).
    MultiDocumentFound,

    /// YAML contains duplicate keys (forbidden).
    DuplicateKey { key: String },

    /// YAML contains float values (forbidden).
    FloatNotAllowed { value: String },

    /// Integer outside safe range.
    IntegerOutOfRange { value: i64 },

    /// Nesting too deep.
    MaxDepthExceeded { depth: usize },

    /// Too many keys in mapping.
    MaxKeysExceeded { count: usize },

    /// String too long.
    StringTooLong { length: usize },

    /// Input too large.
    InputTooLarge { size: usize },

    /// YAML parse error.
    ParseError { message: String },

    /// JSON serialization error.
    SerializeError { message: String },
}

impl std::fmt::Display for CanonicalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnchorFound { position } => write!(f, "YAML anchor found at {}", position),
            Self::AliasFound { position } => write!(f, "YAML alias found at {}", position),
            Self::TagFound { tag } => write!(f, "YAML tag not allowed: {}", tag),
            Self::MultiDocumentFound => write!(f, "multi-document YAML not allowed"),
            Self::DuplicateKey { key } => write!(f, "duplicate key: {}", key),
            Self::FloatNotAllowed { value } => write!(f, "float values not allowed: {}", value),
            Self::IntegerOutOfRange { value } => {
                write!(f, "integer {} out of safe range (±2^53)", value)
            }
            Self::MaxDepthExceeded { depth } => {
                write!(f, "nesting depth {} exceeds limit {}", depth, MAX_DEPTH)
            }
            Self::MaxKeysExceeded { count } => write!(
                f,
                "mapping has {} keys, exceeds limit {}",
                count, MAX_KEYS_PER_MAPPING
            ),
            Self::StringTooLong { length } => write!(
                f,
                "string length {} exceeds limit {}",
                length, MAX_STRING_LENGTH
            ),
            Self::InputTooLarge { size } => {
                write!(f, "input size {} exceeds limit {}", size, MAX_TOTAL_SIZE)
            }
            Self::ParseError { message } => write!(f, "YAML parse error: {}", message),
            Self::SerializeError { message } => write!(f, "JSON serialize error: {}", message),
        }
    }
}

impl std::error::Error for CanonicalizeError {}

/// Result type for canonicalization operations.
pub type CanonicalizeResult<T> = Result<T, CanonicalizeError>;

impl From<CanonicalizeError> for RegistryError {
    fn from(err: CanonicalizeError) -> Self {
        RegistryError::InvalidResponse {
            message: format!("canonicalization failed: {}", err),
        }
    }
}
