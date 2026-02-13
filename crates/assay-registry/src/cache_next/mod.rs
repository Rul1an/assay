//! Wave4 Step2 cache split scaffold.
//!
//! Commit A contract:
//! - `cache.rs` remains the active facade.
//! - No wiring to this module yet.
//! - No behavior/perf changes in this commit.

pub(crate) mod errors;
pub(crate) mod integrity;
pub(crate) mod io;
pub(crate) mod keys;
pub(crate) mod policy;
pub(crate) mod tests;
