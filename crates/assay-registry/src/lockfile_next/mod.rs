//! Wave4 Step2 lockfile split scaffold.
//!
//! Commit A contract:
//! - `lockfile.rs` remains the active facade.
//! - No wiring to this module yet.
//! - No behavior/perf changes in this commit.

pub(crate) mod digest;
pub(crate) mod errors;
pub(crate) mod format;
pub(crate) mod parse;
pub(crate) mod tests;
pub(crate) mod types;
