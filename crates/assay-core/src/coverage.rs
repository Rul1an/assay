//! Coverage metrics for Assay policies.
//!
//! This facade keeps the public `assay_core::coverage` API stable while the
//! implementation is split into smaller reviewable modules.

#[path = "coverage_next/mod.rs"]
mod coverage_next;

pub use coverage_next::{
    CoverageAnalyzer, CoverageReport, HighRiskGap, PolicyViolation, PolicyWarning, RuleCoverage,
    ToolCoverage, TraceRecord,
};
