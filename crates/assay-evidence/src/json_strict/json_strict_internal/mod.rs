//! Wave7C Step2 scaffold for json_strict split.
//!
//! Responsibility boundaries (to be enforced by reviewer gates):
//! - validate.rs: JsonValidator/state-machine flow
//! - decode.rs: JSON string decoding/unescape boundary
//! - limits.rs: limit constants/helpers boundary
//! - run.rs: optional orchestration entrypoint (layout-tolerant gate)

pub(crate) mod decode;
pub(crate) mod limits;
pub(crate) mod run;
pub(crate) mod validate;

#[cfg(test)]
mod tests;
