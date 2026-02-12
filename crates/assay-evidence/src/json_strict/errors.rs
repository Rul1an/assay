//! Error types and DoS protection limits for strict JSON parsing.

use thiserror::Error;

// DoS protection limits (public for tests)
pub const MAX_NESTING_DEPTH: usize = 64;
pub const MAX_KEYS_PER_OBJECT: usize = 10_000;
pub const MAX_STRING_LENGTH: usize = 1_048_576; // 1MB

/// Error returned when strict JSON parsing fails.
#[derive(Debug, Error)]
pub enum StrictJsonError {
    #[error("Duplicate key '{key}' at path '{path}'")]
    DuplicateKey { key: String, path: String },

    #[error("Invalid unicode escape sequence at position {position}")]
    InvalidUnicodeEscape { position: usize },

    #[error("Lone surrogate at position {position}: {codepoint}")]
    LoneSurrogate { position: usize, codepoint: String },

    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Security limit exceeded: nesting depth {depth} exceeds maximum {MAX_NESTING_DEPTH}")]
    NestingTooDeep { depth: usize },

    #[error(
        "Security limit exceeded: {count} keys in object exceeds maximum {MAX_KEYS_PER_OBJECT}"
    )]
    TooManyKeys { count: usize },

    #[error("Security limit exceeded: string length {length} exceeds maximum {MAX_STRING_LENGTH}")]
    StringTooLong { length: usize },
}
