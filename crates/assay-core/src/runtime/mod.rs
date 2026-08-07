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

// `pub(crate)` so the tool-pattern matcher below it is reachable from the divergence test in
// `mcp::policy::matcher`. Its `authorizer_internal::policy::glob_matches_impl` was already marked
// `pub(crate)`, which meant nothing behind a private parent; this makes that marker true.
pub(crate) mod authorizer;
mod mandate_store;
mod schema;

pub use authorizer::{
    AuthorizeError, Authorizer, AuthzConfig, MandateData, MandateKind, OperationClass, PolicyError,
    ToolCallData, DEFAULT_CLOCK_SKEW_SECONDS,
};
pub use mandate_store::{
    compute_use_id, AuthzError, AuthzReceipt, ConsumeParams, MandateMetadata, MandateStore,
};
pub use schema::MANDATE_SCHEMA;
