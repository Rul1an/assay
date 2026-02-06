//! summary.json output per SPEC-PR-Gate-Outputs-v1
//!
//! This module defines the machine-readable summary format for `assay ci` and `assay run`.
//! The summary includes schema versioning, exit/reason codes, provenance, and results.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Serde helpers: serialize Option<u64> as JSON string or null to avoid precision loss (u64 > 2^53 in JS).
mod serde_seed {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize_opt_u64_as_str<S>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match v {
            Some(n) => s.serialize_str(&n.to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize_opt_u64_from_str<'de, D>(d: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<serde_json::Value> = Option::deserialize(d)?;
        match opt {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(s)) => {
                let n = s.parse::<u64>().map_err(serde::de::Error::custom)?;
                Ok(Some(n))
            }
            Some(serde_json::Value::Number(num)) => {
                // Legacy only; write path always emits string.
                let n = num
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("seed number must be u64"))?;
                Ok(Some(n))
            }
            Some(other) => Err(serde::de::Error::custom(format!(
                "seed must be string or null, got: {other}"
            ))),
        }
    }
}

/// Current schema version for summary.json
pub const SCHEMA_VERSION: u32 = 1;

/// Reason code registry version (stable for downstream branching).
/// Downstream MUST branch on (reason_code_version, reason_code) rather than exit code.
pub const REASON_CODE_VERSION: u32 = 1;

/// Seed version for deterministic replay (E7.2). Same philosophy as reason_code_version.
pub const SEED_VERSION: u32 = 1;

/// Machine-readable summary for the PR gate
///
/// See: SPEC-PR-Gate-Outputs-v1.md for the full contract.
/// Downstream MUST branch on (reason_code_version, reason_code) rather than exit code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Schema version for compatibility detection
    pub schema_version: u32,

    /// Version of the reason code registry. MUST be 1 for Outputs-v1. Downstream MUST branch on (reason_code_version, reason_code) rather than exit code.
    pub reason_code_version: u32,

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

    /// Seeds for deterministic replay (E7.2). Always present for schema stability (order_seed/judge_seed null when unknown).
    pub seeds: Seeds,

    /// Judge reliability metrics (E7.3). Present when run had judge evaluations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_metrics: Option<JudgeMetrics>,

    /// SARIF truncation (E2.3). Present when SARIF was truncated (N results omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sarif: Option<SarifOutputInfo>,
}

/// SARIF output metadata (E2.3). Written when SARIF was truncated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifOutputInfo {
    /// Number of results omitted from SARIF due to max_results limit.
    pub omitted: u64,
}

/// Seeds used in the run (replay determinism). Always present in Summary; order_seed/judge_seed encoded as string or null to avoid JSON number precision loss (u64 > 2^53).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seeds {
    /// Version of the seed schema; consumers MUST branch on this.
    pub seed_version: u32,
    /// Seed used for test execution order (shuffle). Serialized as decimal string or null (schema stability + consumer-safe).
    #[serde(
        serialize_with = "serde_seed::serialize_opt_u64_as_str",
        deserialize_with = "serde_seed::deserialize_opt_u64_from_str"
    )]
    pub order_seed: Option<u64>,
    /// Seed used for judge randomization (per-test seed derived from suite seed when present). MAY be null until implemented; consumers MUST handle null.
    #[serde(
        serialize_with = "serde_seed::serialize_opt_u64_as_str",
        deserialize_with = "serde_seed::deserialize_opt_u64_from_str"
    )]
    pub judge_seed: Option<u64>,
    /// Optional: determinism for telemetry sampling (future use).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_seed: Option<u64>,
}

impl Default for Seeds {
    fn default() -> Self {
        Self {
            seed_version: SEED_VERSION,
            order_seed: None,
            judge_seed: None,
            sampling_seed: None,
        }
    }
}

/// Judge reliability metrics (low cardinality, E8-consistent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeMetrics {
    /// Fraction of judge evaluations that returned Abstain (uncertain).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstain_rate: Option<f64>,
    /// Fraction of evaluations where order was swapped and outcome differed (flip).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flip_rate: Option<f64>,
    /// Fraction of evaluations where all samples agreed (consensus).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_rate: Option<f64>,
    /// Count of runs where judge was unavailable (infra/transport); do not count toward abstain_rate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_count: Option<u32>,
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

    /// True when output is from replaying a bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<bool>,

    /// SHA256 digest of replay bundle archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,

    /// Replay mode: offline|live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_mode: Option<String>,

    /// Optional original run id from source run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
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
            replay: None,
            bundle_digest: None,
            replay_mode: None,
            source_run_id: None,
        }
    }
}

impl Summary {
    /// Create a success summary
    pub fn success(assay_version: &str, verify_enabled: bool) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            reason_code_version: REASON_CODE_VERSION,
            exit_code: 0,
            reason_code: String::new(),
            message: Some("All tests passed".to_string()),
            next_step: None,
            provenance: Provenance::new(assay_version, verify_enabled),
            results: None,
            performance: None,
            seeds: Seeds::default(),
            judge_metrics: None,
            sarif: None,
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
            reason_code_version: REASON_CODE_VERSION,
            exit_code,
            reason_code: reason_code.to_string(),
            message: Some(message.to_string()),
            next_step: Some(next_step.to_string()),
            provenance: Provenance::new(assay_version, verify_enabled),
            results: None,
            performance: None,
            seeds: Seeds::default(),
            judge_metrics: None,
            sarif: None,
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

    /// Set replay provenance fields (E9c).
    pub fn with_replay_provenance(
        mut self,
        bundle_digest: String,
        replay_mode: &str,
        source_run_id: Option<String>,
    ) -> Self {
        self.provenance.replay = Some(true);
        self.provenance.bundle_digest = Some(bundle_digest);
        self.provenance.replay_mode = Some(replay_mode.to_string());
        self.provenance.source_run_id = source_run_id;
        self
    }

    /// Set seeds for replay determinism (E7.2). Keys always present in JSON (string or null).
    pub fn with_seeds(mut self, order_seed: Option<u64>, judge_seed: Option<u64>) -> Self {
        self.seeds.order_seed = order_seed;
        self.seeds.judge_seed = judge_seed;
        self
    }

    /// Set judge reliability metrics (E7.3)
    pub fn with_judge_metrics(mut self, metrics: JudgeMetrics) -> Self {
        self.judge_metrics = Some(metrics);
        self
    }

    /// Set SARIF truncation info (E2.3). Call when omitted_count > 0.
    pub fn with_sarif_omitted(mut self, omitted: u64) -> Self {
        if omitted > 0 {
            self.sarif = Some(SarifOutputInfo { omitted });
        }
        self
    }
}

/// Compute judge reliability metrics from run results (E7.3).
/// Returns None if no results have judge details.
/// One test can contribute multiple evaluations (one per metric name, e.g. faithfulness + relevance); rates are per-evaluation.
pub fn judge_metrics_from_results(results: &[crate::model::TestResultRow]) -> Option<JudgeMetrics> {
    use crate::model::TestStatus;

    let mut total_judge = 0u32;
    let mut abstain_count = 0u32;
    let mut consensus_count = 0u32;
    let mut flip_count = 0u32;

    for r in results {
        let Some(metrics) = r.details.get("metrics").and_then(|m| m.as_object()) else {
            continue;
        };
        for (_name, metric_val) in metrics {
            let Some(details) = metric_val.get("details") else {
                continue;
            };
            let verdict = details.get("verdict").and_then(|v| v.as_str());
            let agreement = details.get("agreement").and_then(|v| v.as_f64());
            let swapped = details
                .get("swapped")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if verdict.is_none() && agreement.is_none() {
                continue;
            }
            total_judge += 1;

            if verdict == Some("Abstain") {
                abstain_count += 1;
            }
            if let Some(a) = agreement {
                if a == 0.0 || a == 1.0 {
                    consensus_count += 1;
                }
                // flip_rate: heuristic proxy for "order was swapped and outcome differed".
                // We do not store the counterfactual verdict, so we use: swapped + non-unanimous
                // (0 < agreement < 1). This does NOT guarantee the verdict actually flipped;
                // it indicates order may have affected outcome. Strict definition would require
                // the judge to record whether pass/fail differed under the other ordering.
                if swapped && a > 0.0 && a < 1.0 {
                    flip_count += 1;
                }
            }
        }
    }

    if total_judge == 0 {
        return None;
    }

    let total = total_judge as f64;
    Some(JudgeMetrics {
        abstain_rate: Some(abstain_count as f64 / total),
        flip_rate: Some(flip_count as f64 / total),
        consensus_rate: Some(consensus_count as f64 / total),
        unavailable_count: Some(
            results
                .iter()
                .filter(|r| matches!(r.status, TestStatus::Error))
                .filter(|r| {
                    let m = r.message.to_lowercase();
                    m.contains("timeout")
                        || m.contains("500")
                        || m.contains("502")
                        || m.contains("503")
                        || m.contains("504")
                        || m.contains("rate limit")
                        || m.contains("network")
                })
                .count() as u32,
        ),
    })
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
        assert_eq!(summary.reason_code_version, 1);
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

        assert_eq!(summary.reason_code_version, 1);
        assert_eq!(summary.exit_code, 2);
        assert_eq!(summary.reason_code, "E_TRACE_NOT_FOUND");
        assert!(summary.next_step.is_some());
    }

    #[test]
    fn test_summary_serialization() {
        let summary = Summary::success("2.12.0", true).with_results(5, 2, 7);

        let json = serde_json::to_string_pretty(&summary).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"reason_code_version\": 1"));
        assert!(json.contains("\"assay_version\": \"2.12.0\""));

        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["reason_code_version"], 1,
            "reason_code_version must be present and integer"
        );

        // E7.2: seeds always present; order_seed/judge_seed keys exist (string or null)
        assert_eq!(v["seeds"]["seed_version"], 1);
        assert!(
            v["seeds"].get("order_seed").is_some(),
            "order_seed key must exist"
        );
        assert!(
            v["seeds"].get("judge_seed").is_some(),
            "judge_seed key must exist"
        );
        assert!(v["seeds"]["order_seed"].is_null());
        assert!(v["seeds"]["judge_seed"].is_null());
    }

    #[test]
    fn test_seeds_serialize_as_string() {
        let summary = Summary::success("2.12.0", true)
            .with_results(1, 0, 1)
            .with_seeds(Some(17390767342376325021), None);

        let json = serde_json::to_string(&summary).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            v["seeds"]["order_seed"].is_string(),
            "order_seed must be string to avoid precision loss"
        );
        assert_eq!(
            v["seeds"]["order_seed"].as_str(),
            Some("17390767342376325021")
        );
        assert!(v["seeds"]["judge_seed"].is_null());
    }

    #[test]
    fn test_judge_metrics_abstain_not_counted_as_unavailable() {
        use crate::model::{TestResultRow, TestStatus};

        // Rows with verdict Abstain (uncertain) must NOT increment unavailable_count.
        // unavailable_count is only for Error status + infra message (timeout/5xx/rate limit/network).
        let results = vec![TestResultRow {
            test_id: "t1".into(),
            status: TestStatus::Pass,
            score: Some(0.5),
            cached: false,
            message: String::new(),
            details: serde_json::json!({
                "metrics": {
                    "m1": { "details": { "verdict": "Abstain", "agreement": 0.5 } }
                }
            }),
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        }];
        let metrics = judge_metrics_from_results(&results).unwrap();
        assert_eq!(metrics.abstain_rate, Some(1.0));
        assert_eq!(metrics.unavailable_count, Some(0));
    }
}
