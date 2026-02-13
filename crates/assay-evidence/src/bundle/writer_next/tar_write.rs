//! Deterministic tar.gz write boundary for Step-3 split.
//!
//! Target responsibilities:
//! - deterministic tar header configuration
//! - deterministic gzip metadata (e.g., mtime)
//!
//! Forbidden responsibilities:
//! - verify/read flow
