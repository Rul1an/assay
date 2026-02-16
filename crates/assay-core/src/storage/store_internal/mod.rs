//! Internal split scaffold for storage store.
//!
//! Commit A (Wave7B Step2): structure only, no behavior wiring.

pub(crate) mod cache;
pub(crate) mod embeddings;
pub(crate) mod episodes;
pub(crate) mod helpers;
pub(crate) mod quarantine;
pub(crate) mod results;
pub(crate) mod runs;
pub(crate) mod schema;

#[cfg(test)]
pub(crate) mod tests;
