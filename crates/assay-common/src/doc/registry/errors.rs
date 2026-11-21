//! Canonicalization error types and limits.

use std::string::String;

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

/// Refusals from the strict wire reader, translated at the registry API boundary.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadError {
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
}

pub type ReadResult<T> = Result<T, ReadError>;
