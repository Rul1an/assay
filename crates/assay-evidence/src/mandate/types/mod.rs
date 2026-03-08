//! Mandate Evidence Types (SPEC-Mandate-v1)
//!
//! Core data structures for cryptographically-signed user authorization
//! evidence for AI agent tool calls.

mod core;
mod schema;
pub(crate) mod serde;

#[cfg(test)]
mod tests;

pub use core::{
    AuthMethod, Constraints, Context, Mandate, MandateBuilder, MandateContent, MandateKind,
    MaxValue, OperationClass, Principal, Scope, Signature, Validity,
};
pub use schema::{MANDATE_PAYLOAD_TYPE, MANDATE_REVOKED_PAYLOAD_TYPE, MANDATE_USED_PAYLOAD_TYPE};
