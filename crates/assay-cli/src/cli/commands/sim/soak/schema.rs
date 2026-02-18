//! ADR-025 I1 Step2 (B2) schema validation skeleton.
//!
//! Commit 1 intentionally keeps this module compile-safe and non-behavioral.

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value;

/// Placeholder validator; real implementation lands in Commit 2.
pub(crate) fn validate_soak_report_v1(_instance: &Value) -> Result<()> {
    Ok(())
}
