//! Soak testing — N runs of policy evaluation, pass^k semantics (ADR-025 E1).
//!
//! Report schema: soak-report-v1
//!
//! Schema versioning: v1 allows additive changes within the same major. Breaking changes => v2.
//! soak_mode added in E1.1; seed_strategy added for E1.2 forward-compat.
//!
//! variation_source contract strings (stable; tooling may filter on these):
//! - deterministic_repeat: artifact-mode, N× same input
//! - run_trajectories: run-mode, N× new bundle per iteration
//! - seed_sweep: run-mode + per_iteration seed (E1.2)

pub const VAR_SRC_DETERMINISTIC_REPEAT: &str = "deterministic_repeat";
pub const VAR_SRC_RUN_TRAJECTORIES: &str = "run_trajectories";

use assay_evidence::VerifyLimits;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Soak report v1 (ADR-025).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoakReport {
    pub schema_version: String,
    pub mode: String,
    /// Soak mode: "artifact" = N× same bundle; "run" = N× new bundle (variance/drift).
    pub soak_mode: String,
    /// Source of run-to-run variation. "deterministic_repeat" = N× same input (MVP).
    pub variation_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assay_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    pub iterations: u32,
    pub seed: u64,
    /// Seed derivation: "fixed" (artifact) | "per_iteration" (run-mode E1.2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_strategy: Option<String>,
    pub time_budget_secs: u64,
    /// "soak" = global budget for entire run (not per-run).
    pub time_budget_scope: String,
    pub limits: SoakLimits,
    pub packs: Vec<PackRef>,
    pub decision_policy: DecisionPolicy,
    pub results: SoakResults,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<RunResult>>,
}

/// Limits used during soak (mirrors VerifyLimits).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoakLimits {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: u64,
    pub max_line_bytes: u64,
    pub max_path_len: u64,
    pub max_json_depth: u64,
}

impl From<VerifyLimits> for SoakLimits {
    fn from(l: VerifyLimits) -> Self {
        Self {
            max_bundle_bytes: l.max_bundle_bytes,
            max_decode_bytes: l.max_decode_bytes,
            max_manifest_bytes: l.max_manifest_bytes,
            max_events_bytes: l.max_events_bytes,
            max_events: l.max_events as u64,
            max_line_bytes: l.max_line_bytes as u64,
            max_path_len: l.max_path_len as u64,
            max_json_depth: l.max_json_depth as u64,
        }
    }
}

/// Pack reference for report. digest required for attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRef {
    pub name: String,
    pub version: String,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Decision policy (pass/fail threshold).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionPolicy {
    pub pass_on_severity_at_or_above: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_on_first_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failures: Option<u32>,
}

/// Aggregated soak results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoakResults {
    pub runs: u32,
    pub passes: u32,
    pub failures: u32,
    pub infra_errors: u32,
    pub pass_rate: f64,
    pub pass_all: bool,
    /// 1-based index of first policy failure (findings ≥ threshold).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_policy_failure_at: Option<u32>,
    /// 1-based index of first infra error (verify failed, budget exceeded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_infra_error_at: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violations_by_rule: Option<BTreeMap<String, u32>>,
    /// Always present when infra_errors > 0 (time_budget_exceeded, verification_failed, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infra_errors_by_kind: Option<BTreeMap<String, u32>>,
}

/// Per-run result (optional detail).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    pub index: u32,
    pub status: String, // "pass" | "fail" | "infra_error"
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violated_rules: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infra_error_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infra_error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn soak_report_schema_v1_serialization() {
        let report = SoakReport {
            schema_version: "soak-report-v1".into(),
            mode: "soak".into(),
            soak_mode: "artifact".into(),
            variation_source: VAR_SRC_DETERMINISTIC_REPEAT.into(),
            generated_at: Some("2026-02-11T12:00:00Z".into()),
            assay_version: Some("2.18.0".into()),
            suite: None,
            iterations: 5,
            seed: 42,
            seed_strategy: Some("fixed".into()),
            time_budget_secs: 60,
            time_budget_scope: "soak".into(),
            limits: SoakLimits {
                max_bundle_bytes: 100_000_000,
                max_decode_bytes: 1_000_000_000,
                max_manifest_bytes: 10_000_000,
                max_events_bytes: 500_000_000,
                max_events: 100_000,
                max_line_bytes: 1_000_000,
                max_path_len: 256,
                max_json_depth: 64,
            },
            packs: vec![PackRef {
                name: "cicd-starter".into(),
                version: "1.0.0".into(),
                digest: "sha256:abc123".into(),
                kind: Some("quality".into()),
                source: Some("builtin:cicd-starter".into()),
            }],
            decision_policy: DecisionPolicy {
                pass_on_severity_at_or_above: "error".into(),
                stop_on_first_failure: None,
                max_failures: None,
            },
            results: SoakResults {
                runs: 5,
                passes: 4,
                failures: 1,
                infra_errors: 0,
                pass_rate: 0.8,
                pass_all: false,
                first_policy_failure_at: Some(3),
                first_infra_error_at: None,
                violations_by_rule: {
                    let mut m = BTreeMap::new();
                    m.insert("cicd-starter@1.0.0:CICD-004".into(), 1);
                    Some(m)
                },
                infra_errors_by_kind: None,
            },
            runs: None,
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: SoakReport = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.schema_version, "soak-report-v1");
        assert_eq!(parsed.mode, "soak");
        assert_eq!(parsed.soak_mode, "artifact");
        assert_eq!(parsed.variation_source, "deterministic_repeat");
        assert_eq!(parsed.results.pass_rate, 0.8);
        assert!(!parsed.results.pass_all);
        assert_eq!(
            parsed.results.violations_by_rule.as_ref().unwrap()["cicd-starter@1.0.0:CICD-004"],
            1
        );
    }
}
