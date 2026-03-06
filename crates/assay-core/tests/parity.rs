//! Parity Tests: Batch vs Streaming Mode
//!
//! These tests verify that the same policy + trace produces identical results
//! whether evaluated in batch mode (`assay run`) or streaming mode (`assay-mcp-server`).
//!
//! This is critical for the "one engine, two modes" architecture guarantee.
//!
//! Run with:
//!   cargo test -p assay-core --test parity -- --nocapture
//!
//! CI gate:
//!   Any parity failure is a BLOCKER for release.

#[path = "parity/assertions.rs"]
mod assertions;
#[path = "parity/core_types.rs"]
mod core_types;
#[cfg(test)]
#[path = "parity/parity_contract.rs"]
mod parity_contract;

#[path = "parity/batch.rs"]
pub mod batch;
#[path = "parity/fixtures.rs"]
pub mod fixtures;
#[path = "parity/shared.rs"]
pub mod shared;
#[path = "parity/streaming.rs"]
pub mod streaming;

pub use assertions::{compute_result_hash, verify_parity, ParityResult};
pub use core_types::{CheckInput, CheckResult, CheckType, Outcome, PolicyCheck, ToolCall};
