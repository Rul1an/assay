//! Step-2 split implementation modules for `engine::runner`.
//!
//! `src/engine/runner.rs` remains the stable facade; implementation blocks are
//! mechanically moved here.

pub(crate) mod baseline;
pub(crate) mod cache;
pub(crate) mod errors;
pub(crate) mod execute;
pub(crate) mod retry;
pub(crate) mod scoring;
pub(crate) mod tests;
