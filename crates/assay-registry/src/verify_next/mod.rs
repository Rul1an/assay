//! Step-2 scaffold for verify module split.
//!
//! Commit A keeps `src/verify.rs` as the active implementation to guarantee
//! zero behavior change while the new module layout is prepared.
//!
//! This module is not wired into `lib.rs` yet.
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
