//! summary.json output per SPEC-PR-Gate-Outputs-v1
//!
//! This module defines the machine-readable summary format for `assay ci` and `assay run`.
//! The summary includes schema versioning, exit/reason codes, provenance, and results.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current schema version for summary.json
pub const SCHEMA_VERSION: u32 = 1;

/// Machine-readable summary for the PR gate
///
/// See: SPEC-PR-Gate-Outputs-v1.md for the full contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Schema version for compatibility detection
    pub schema_version: u32,

    /// Exit code: 0=pass, 1=test failure, 2=config error, 3=infra error
    pub exit_code: i32,

    /// Stable machine-readable reason code (e.g., "E_TRACE_NOT_FOUND")
    pub reason_code: String,

    /// Human-readable message describing the outcome
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Suggested next step when exit_code != 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,

    /// Provenance information for auditability
    pub provenance: Provenance,

    /// Results summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<ResultsSummary>,

    /// Performance metrics (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<PerformanceMetrics>,
}

/// Provenance fields for artifact auditability (ADR-019 P0.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Assay CLI version that produced this run
    pub assay_version: String,

    /// Verification mode: "enabled" or "disabled"
    pub verify_mode: String,

    /// Digest of policy/pack used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_pack_digest: Option<String>,

    /// Digest of baseline used for comparison
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_digest: Option<String>,

    /// Digest of trace input (optional for privacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_digest: Option<String>,
}

/// Test results summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultsSummary {
    /// Count of tests passed
    pub passed: usize,

    /// Count of tests failed
    pub failed: usize,

    /// Count of tests with warnings/flaky
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warned: Option<usize>,

    /// Count of tests skipped (e.g., cache hit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<usize>,

    /// Total test count
    pub total: usize,
}

/// Performance metrics for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total run duration in milliseconds
    pub total_duration_ms: u64,

    /// Cache hit rate (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit_rate: Option<f64>,

    /// Slowest tests (up to 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowest_tests: Option<Vec<SlowestTest>>,

    /// Phase timings (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_timings: Option<PhaseTimings>,
}

/// Entry for slowest tests list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowestTest {
    pub test_id: String,
    pub duration_ms: u64,
}

/// Phase timing breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTimings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingest_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_ms: Option<u64>,
}

impl Provenance {
    /// Create a new Provenance with version and verify mode
    fn new(assay_version: &str, verify_enabled: bool) -> Self {
        Self {
            assay_version: assay_version.to_string(),
            verify_mode: if verify_enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
            policy_pack_digest: None,
            baseline_digest: None,
            trace_digest: None,
        }
    }
}

impl Summary {
    /// Create a success summary
    pub fn success(assay_version: &str, verify_enabled: bool) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            exit_code: 0,
            reason_code: String::new(),
            message: Some("All tests passed".to_string()),
            next_step: None,
            provenance: Provenance::new(assay_version, verify_enabled),
            results: None,
            performance: None,
        }
    }

    /// Create a failure summary with reason code and next step
    pub fn failure(
        exit_code: i32,
        reason_code: &str,
        message: &str,
        next_step: &str,
        assay_version: &str,
        verify_enabled: bool,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            exit_code,
            reason_code: reason_code.to_string(),
            message: Some(message.to_string()),
            next_step: Some(next_step.to_string()),
            provenance: Provenance::new(assay_version, verify_enabled),
            results: None,
            performance: None,
        }
    }

    /// Set results summary
    pub fn with_results(mut self, passed: usize, failed: usize, total: usize) -> Self {
        self.results = Some(ResultsSummary {
            passed,
            failed,
            warned: None,
            skipped: None,
            total,
        });
        self
    }

    /// Set performance metrics
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.performance = Some(PerformanceMetrics {
            total_duration_ms: duration_ms,
            cache_hit_rate: None,
            slowest_tests: None,
            phase_timings: None,
        });
        self
    }

    /// Set provenance digests
    pub fn with_digests(
        mut self,
        policy_digest: Option<String>,
        baseline_digest: Option<String>,
        trace_digest: Option<String>,
    ) -> Self {
        self.provenance.policy_pack_digest = policy_digest;
        self.provenance.baseline_digest = baseline_digest;
        self.provenance.trace_digest = trace_digest;
        self
    }
}

/// Write summary.json to file
pub fn write_summary(summary: &Summary, out: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(summary)?;
    std::fs::write(out, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_summary() {
        let summary = Summary::success("2.12.0", true)
            .with_results(10, 0, 10)
            .with_duration(1234);

        assert_eq!(summary.schema_version, 1);
        assert_eq!(summary.exit_code, 0);
        assert_eq!(summary.reason_code, "");
        assert_eq!(summary.provenance.verify_mode, "enabled");
    }

    #[test]
    fn test_failure_summary() {
        let summary = Summary::failure(
            2,
            "E_TRACE_NOT_FOUND",
            "Trace file not found: traces/ci.jsonl",
            "Run: assay doctor --config ci-eval.yaml",
            "2.12.0",
            true,
        );

        assert_eq!(summary.exit_code, 2);
        assert_eq!(summary.reason_code, "E_TRACE_NOT_FOUND");
        assert!(summary.next_step.is_some());
    }

    #[test]
    fn test_summary_serialization() {
        let summary = Summary::success("2.12.0", true).with_results(5, 2, 7);

        let json = serde_json::to_string_pretty(&summary).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"assay_version\": \"2.12.0\""));
    }
}
