//! Runtime mandate enforcement.
//!
//! This module provides runtime authorization and consumption of mandates
//! for tool calls. It ensures atomic single-use enforcement, nonce replay
//! prevention, and idempotent consumption.
//!
//! ## Architecture (SPEC-Mandate-v1.0.3 §7)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        MCP Proxy                                │
//! │  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐  │
//! │  │ Policy Check │───▶│ Authorizer   │───▶│ Forward to Tool  │  │
//! │  └──────────────┘    └──────┬───────┘    └────────┬─────────┘  │
//! │                             │                      │            │
//! │                     ┌───────▼───────┐      ┌──────▼──────┐     │
//! │                     │ MandateStore  │      │ Tool Server │     │
//! │                     │   (SQLite)    │      └─────────────┘     │
//! │                     └───────────────┘                          │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod authorizer;
mod mandate_store;
mod schema;

/// One item, under `cfg(test)` only, for the divergence test in `mcp::policy::matcher`.
///
/// The first version of this made `mod authorizer` and `mod authorizer_internal` both
/// `pub(crate)`, which reached the matcher by exposing the authorizer's entire internal surface
/// — the mandate store, the consume path, the policy evaluator — in release builds, to get one
/// function into one test. A re-export costs the crate exactly what the test needs.
#[cfg(test)]
pub(crate) use authorizer::glob_matches_impl;

pub use authorizer::{
    AuthorizeError, Authorizer, AuthzConfig, MandateData, MandateKind, OperationClass, PolicyError,
    ToolCallData, DEFAULT_CLOCK_SKEW_SECONDS,
};
pub use mandate_store::{
    compute_use_id, AuthzError, AuthzReceipt, ConsumeParams, MandateMetadata, MandateStore,
};
pub use schema::MANDATE_SCHEMA;
