//! Bundle verify orchestration boundary for Step-3 split.
//!
//! Target responsibilities:
//! - verify_bundle flow orchestration
//! - calls into events/tar_read/limits/errors helpers
//!
//! Forbidden responsibilities:
//! - write path orchestration
//! - deterministic tar/gzip write settings
