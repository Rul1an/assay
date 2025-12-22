//! Verdict Core - LLM evaluation engine with actionable diagnostics.
//!
//! This crate provides the core functionality for Verdict, including:
//! - Diagnostic system with stable error codes and actionable fix steps
//! - Trace verification with closest match hints
//! - String similarity for fuzzy matching
//!
//! # Error Handling
//!
//! All errors in Verdict include:
//! - A stable error code (E001, E002, etc.) for documentation references
//! - Actionable fix steps (1-3 bullets)
//! - Rich context for debugging
//!
//! ## Example: Handling a trace miss
//!
//! ```rust
//! use verdict_core::errors::{Diagnostic, DiagnosticCode, DiagnosticContext, ClosestMatch};
//! use verdict_core::trace::{TraceVerifier, TestCase};
//!
//! // When a trace entry doesn't match, the verifier provides helpful hints
//! let verifier = TraceVerifier::new();
//! let diagnostics = verifier.verify_with_diagnostics(&tests, &traces, false, false);
//!
//! for diag in diagnostics {
//!     // Terminal output with colors
//!     eprintln!("{}", diag.format_terminal());
//!     
//!     // Or plain text for logs/CI
//!     eprintln!("{}", diag.format_plain());
//! }
//! ```
//!
//! ## Error Code Reference
//!
//! | Code | Category | Description |
//! |------|----------|-------------|
//! | E001 | Trace | No matching trace entry found |
//! | E002 | Trace | Trace file not found |
//! | E003 | Trace | Trace schema version mismatch |
//! | E020 | Baseline | Baseline file not found |
//! | E021 | Baseline | Suite name mismatch |
//! | E022 | Baseline | Schema version mismatch |
//! | E040 | Embedding | Dimensions mismatch |
//! | E041 | Embedding | Model ID mismatch |
//! | E042 | Embedding | Not precomputed for strict replay |
//! | E060 | Judge | Not precomputed for strict replay |
//! | E062 | Judge | Disagreement (voting failed) |
//! | E080 | Config | Config file not found |
//! | E081 | Config | Parse error (invalid YAML) |
//! | E100 | Runtime | Strict replay violation |
//! | E101 | Runtime | Rate limit exceeded |
//! | E120 | Database | Migration failed |

pub mod errors;
pub mod trace;

// Re-export commonly used types
pub use errors::{Diagnostic, DiagnosticCode, DiagnosticContext, DiagnosticResult};
pub use trace::{TestCase, TraceEntry, TraceVerifier, VerifyResult};
