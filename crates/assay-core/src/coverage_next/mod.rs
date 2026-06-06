//! Coverage metrics for Assay policies.
//!
//! Analyzes traces to determine:
//! - Tool coverage: which tools from policy were exercised
//! - Rule coverage: which rules were triggered
//! - Gap detection: high-risk tools never seen in traces

mod analyzer;
mod report;
mod types;

pub use analyzer::CoverageAnalyzer;
pub use types::{
    CoverageReport, HighRiskGap, PolicyViolation, PolicyWarning, RuleCoverage, ToolCoverage,
    TraceRecord,
};

#[cfg(test)]
mod tests;
