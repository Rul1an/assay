//! Runtime mandate enforcement.
//!
//! This module provides runtime authorization and consumption of mandates
//! for tool calls. It ensures atomic single-use enforcement, nonce replay
//! prevention, and idempotent consumption.

mod mandate_store;
mod schema;

pub use mandate_store::{AuthzError, AuthzReceipt, ConsumeParams, MandateMetadata, MandateStore};
pub use schema::MANDATE_SCHEMA;
