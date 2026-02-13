//! Step-2 scaffold for verify module split.
//!
//! `src/verify.rs` remains the public facade and delegates into `verify_next/*`
//! so symbol paths/signatures stay stable during the split rollout.
//!
//! This module is intentionally not exposed via `lib.rs` as `pub mod verify_next`.
//!
//! Forbidden knowledge for this facade:
//! - no direct crypto implementation details
//! - no DSSE parsing/verification internals
//! - no policy branch logic beyond orchestration

pub(crate) mod digest;
pub(crate) mod dsse;
pub(crate) mod errors;
pub(crate) mod keys;
pub(crate) mod policy;
pub(crate) mod tests;
pub(crate) mod wire;
