//! Bundle write orchestration boundary for Step-3 split.
//!
//! Target responsibilities:
//! - BundleWriter flow orchestration
//! - calls into manifest/events/tar_write helpers
//!
//! Forbidden responsibilities:
//! - verify flow decisions
//! - direct limit policy ownership
