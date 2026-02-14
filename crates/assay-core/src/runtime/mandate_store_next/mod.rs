//! Step-2 split implementation modules for `runtime::mandate_store`.
//!
//! `src/runtime/mandate_store.rs` remains the stable facade; implementation
//! blocks are mechanically moved here.

pub(crate) mod consume;
pub(crate) mod revocation;
pub(crate) mod schema;
pub(crate) mod stats;
pub(crate) mod tests;
pub(crate) mod txn;
pub(crate) mod upsert;
