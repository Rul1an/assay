//! Wave7C Step2 scaffold for judge split.
//!
//! Responsibility boundaries (to be enforced by reviewer gates):
//! - run.rs: orchestration/evaluate flow
//! - prompt.rs: prompt builders/constants only
//! - client.rs: judge call + response parse boundary
//! - cache.rs: cache/meta helpers only

pub(crate) mod cache;
pub(crate) mod client;
pub(crate) mod prompt;
pub(crate) mod run;

#[cfg(test)]
mod tests;
