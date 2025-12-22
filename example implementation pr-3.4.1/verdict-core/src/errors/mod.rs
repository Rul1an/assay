//! Error handling with actionable diagnostics.
//!
//! This module provides rich error types that include:
//! - Stable error codes for documentation references
//! - Actionable fix steps
//! - Rich context for debugging
//!
//! # Example
//!
//! ```rust
//! use verdict_core::errors::{Diagnostic, DiagnosticCode, DiagnosticContext};
//!
//! let diag = Diagnostic::new(
//!     DiagnosticCode::E001TraceMiss,
//!     "No matching trace entry found for test 't1'",
//!     DiagnosticContext::TraceMiss {
//!         test_id: "t1".to_string(),
//!         expected_prompt: "Hello world".to_string(),
//!         closest_match: None,
//!     },
//! );
//!
//! // For terminal output with colors
//! eprintln!("{}", diag.format_terminal());
//!
//! // For plain text (logs, CI)
//! eprintln!("{}", diag.format_plain());
//! ```

mod diagnostic;
mod similarity;

pub use diagnostic::{
    ClosestMatch, Diagnostic, DiagnosticCode, DiagnosticContext, DiffPosition,
};
pub use similarity::{
    find_closest_match, find_closest_matches, find_diff_positions, levenshtein_distance,
    similarity_score,
};

/// Result type alias using Diagnostic as the error type.
pub type DiagnosticResult<T> = Result<T, Diagnostic>;

/// Extension trait to convert other errors into Diagnostics.
pub trait IntoDiagnostic<T> {
    /// Convert into a DiagnosticResult with the given error code.
    fn into_diagnostic(
        self,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> DiagnosticResult<T>;
}

impl<T, E: std::error::Error> IntoDiagnostic<T> for Result<T, E> {
    fn into_diagnostic(
        self,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> DiagnosticResult<T> {
        self.map_err(|e| {
            Diagnostic::new(
                code,
                format!("{}: {}", message.into(), e),
                DiagnosticContext::Generic {
                    details: [("original_error".to_string(), e.to_string())]
                        .into_iter()
                        .collect(),
                },
            )
        })
    }
}
