//! Verify split scaffold (Wave5 Step2 Commit B).
//!
//! Layout only: `verify.rs` remains the active implementation in this commit.
//! Function moves and facade delegation are introduced in a follow-up commit.

pub(crate) mod digest;
pub(crate) mod dsse;
pub(crate) mod errors;
pub(crate) mod keys;
pub(crate) mod policy;
pub(crate) mod tests;
pub(crate) mod wire;
