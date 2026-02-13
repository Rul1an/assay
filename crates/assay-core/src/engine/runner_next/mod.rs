//! Step-2 split scaffold modules for `engine::runner`.
//!
//! `src/engine/runner.rs` remains the active implementation during Commit A.
//! These files are placeholders for a later mechanical move in Commit B.

pub(crate) mod baseline;
pub(crate) mod cache;
pub(crate) mod errors;
pub(crate) mod execute;
pub(crate) mod retry;
pub(crate) mod scoring;
pub(crate) mod tests;
