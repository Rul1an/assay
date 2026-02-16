//! Internal split scaffold for pack loader.
//!
//! Commit A (Wave7B Step2): structure only, no behavior wiring.

pub(crate) mod compat;
pub(crate) mod digest;
pub(crate) mod parse;
pub(crate) mod resolve;

#[cfg(test)]
pub(crate) mod tests;
