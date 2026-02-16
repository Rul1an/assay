//! Step-2 split implementation modules for `runtime::authorizer`.
//!
//! `src/runtime/authorizer.rs` remains the stable facade; implementation
//! blocks are mechanically moved here.

pub(crate) mod policy;
pub(crate) mod run;
pub(crate) mod store;

#[cfg(test)]
pub(crate) mod tests;
