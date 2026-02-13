//! Step-2 split scaffold modules for `runtime::mandate_store`.
//!
//! `src/runtime/mandate_store.rs` remains the active implementation during
//! Commit A. This layout is prepared for a later mechanical move in Commit B.

pub(crate) mod consume;
pub(crate) mod revocation;
pub(crate) mod schema;
pub(crate) mod stats;
pub(crate) mod tests;
pub(crate) mod txn;
pub(crate) mod upsert;
