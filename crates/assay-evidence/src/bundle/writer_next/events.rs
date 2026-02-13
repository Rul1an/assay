//! NDJSON/events boundary for Step-3 split.
//!
//! Target responsibilities:
//! - event normalization and line parsing helpers
//! - strict NDJSON validation hooks
//!
//! Forbidden responsibilities:
//! - tar/gzip writer configuration
//! - global limit ownership
