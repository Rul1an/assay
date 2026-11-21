//! Canonicalization error types and limits.

use crate::error::RegistryError;
use assay_common::doc::registry::errors as wire;

/// Maximum nesting depth for YAML structures.
pub const MAX_DEPTH: usize = wire::MAX_DEPTH;

/// Maximum number of keys in a single mapping.
pub const MAX_KEYS_PER_MAPPING: usize = wire::MAX_KEYS_PER_MAPPING;

/// Maximum string length (1MB).
pub const MAX_STRING_LENGTH: usize = wire::MAX_STRING_LENGTH;

/// Maximum total YAML size (10MB).
pub const MAX_TOTAL_SIZE: usize = wire::MAX_TOTAL_SIZE;

/// Maximum safe integer value (2^53 for JSON compatibility).
pub const MAX_SAFE_INTEGER: i64 = wire::MAX_SAFE_INTEGER;

/// Minimum safe integer value (-2^53 for JSON compatibility).
pub const MIN_SAFE_INTEGER: i64 = wire::MIN_SAFE_INTEGER;

/// Errors specific to canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
            message: format!(
                "canonicalization failed (pack invalid/unsupported): {}",
                err
            ),
        }
    }
}

/// Preserve the registry error type at its public boundary.
pub(super) fn from_reader(error: wire::ReadError) -> CanonicalizeError {
    use wire::ReadError as R;
    match error {
        R::AnchorFound { position } => CanonicalizeError::AnchorFound { position },
        R::AliasFound { position } => CanonicalizeError::AliasFound { position },
        R::TagFound { tag } => CanonicalizeError::TagFound { tag },
        R::DuplicateKey { key } => CanonicalizeError::DuplicateKey { key },
        R::FloatNotAllowed { value } => CanonicalizeError::FloatNotAllowed { value },
        R::IntegerOutOfRange { value } => CanonicalizeError::IntegerOutOfRange { value },
        R::MaxDepthExceeded { depth } => CanonicalizeError::MaxDepthExceeded { depth },
        R::MaxKeysExceeded { count } => CanonicalizeError::MaxKeysExceeded { count },
        R::StringTooLong { length } => CanonicalizeError::StringTooLong { length },
        R::InputTooLarge { size } => CanonicalizeError::InputTooLarge { size },
        R::ParseError { message } => CanonicalizeError::ParseError { message },
        R::MultiDocumentFound => CanonicalizeError::MultiDocumentFound,
        _ => CanonicalizeError::ParseError {
            message: "unsupported registry document error".into(),
        },
    }
}
