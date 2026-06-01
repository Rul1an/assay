use serde::{Deserialize, Serialize};

use crate::{
    CgroupCorrelationStatus, KernelLayerStatus, ObservationHealth, ObservationHealthError,
    OBSERVATION_HEALTH_SCHEMA,
};

pub const RUNNER_FIDELITY_VERDICT_SCHEMA: &str = "assay.runner.fidelity_verdict.v0";

pub const PROJECTION_CLAIM_LEVEL_RAW_OBSERVED: &str = "raw_observed";
pub const PROJECTION_CLAIM_LEVEL_PROJECTED_EQUIVALENT: &str = "projected_equivalent";
pub const PROJECTION_CLAIM_LEVEL_INCONCLUSIVE: &str = "inconclusive";

const NON_CLAIMS: &[&str] = &[
    "fidelity_no_observation_health_replacement",
    "fidelity_no_policy_correctness_verdict",
    "fidelity_no_runtime_safety_verdict",
    "fidelity_no_agent_quality_score",
    "fidelity_no_probabilistic_confidence_score",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerFidelityVerdict {
    Clean,
    Clipped,
    CorrelationPartial,
    Failed,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimGateDecision {
    Allowed,
    Degraded,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerClaimGate {
    pub reported_claims: ClaimGateDecision,
    pub measured_positive_claims: ClaimGateDecision,
    pub bounded_negative_claims: ClaimGateDecision,
    pub per_binding_claims: ClaimGateDecision,
}

impl RunnerClaimGate {
    #[must_use]
    pub fn for_verdict(verdict: RunnerFidelityVerdict) -> Self {
        match verdict {
            RunnerFidelityVerdict::Clean => Self {
                reported_claims: ClaimGateDecision::Allowed,
                measured_positive_claims: ClaimGateDecision::Allowed,
                bounded_negative_claims: ClaimGateDecision::Allowed,
                per_binding_claims: ClaimGateDecision::Allowed,
            },
            RunnerFidelityVerdict::Clipped => Self {
                reported_claims: ClaimGateDecision::Allowed,
                measured_positive_claims: ClaimGateDecision::Degraded,
                bounded_negative_claims: ClaimGateDecision::Blocked,
                per_binding_claims: ClaimGateDecision::Allowed,
            },
            RunnerFidelityVerdict::CorrelationPartial => Self {
                reported_claims: ClaimGateDecision::Allowed,
                measured_positive_claims: ClaimGateDecision::Degraded,
                bounded_negative_claims: ClaimGateDecision::Blocked,
                per_binding_claims: ClaimGateDecision::Blocked,
            },
            RunnerFidelityVerdict::NotApplicable => Self {
                reported_claims: ClaimGateDecision::Allowed,
                measured_positive_claims: ClaimGateDecision::Blocked,
                bounded_negative_claims: ClaimGateDecision::Blocked,
                per_binding_claims: ClaimGateDecision::Blocked,
            },
            RunnerFidelityVerdict::Failed => Self {
                reported_claims: ClaimGateDecision::Blocked,
                measured_positive_claims: ClaimGateDecision::Blocked,
                bounded_negative_claims: ClaimGateDecision::Blocked,
                per_binding_claims: ClaimGateDecision::Blocked,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerFidelityReason {
    pub field: String,
    pub observed: String,
    pub rule: String,
}

impl RunnerFidelityReason {
    fn new(field: &str, observed: impl ToString, rule: &str) -> Self {
        Self {
            field: field.to_string(),
            observed: observed.to_string(),
            rule: rule.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerFidelityVerdictReport {
    pub schema: String,
    pub source_schema: String,
    pub run_id: String,
    pub verdict: RunnerFidelityVerdict,
    pub claim_gate: RunnerClaimGate,
    pub reasons: Vec<RunnerFidelityReason>,
    pub non_claims: Vec<String>,
}

impl RunnerFidelityVerdictReport {
    #[must_use]
    pub fn from_observation_health(health: &ObservationHealth) -> Self {
        let mut reasons = Vec::new();
        let verdict = classify_observation_health(health, &mut reasons);
        let mut claim_gate = RunnerClaimGate::for_verdict(verdict);

        if health.cgroup_correlation == CgroupCorrelationStatus::Partial
            && verdict != RunnerFidelityVerdict::Failed
        {
            claim_gate.per_binding_claims = ClaimGateDecision::Blocked;
        }

        Self {
            schema: RUNNER_FIDELITY_VERDICT_SCHEMA.to_string(),
            source_schema: OBSERVATION_HEALTH_SCHEMA.to_string(),
            run_id: health.run_id.clone(),
            verdict,
            claim_gate,
            reasons,
            non_claims: NON_CLAIMS
                .iter()
                .map(|non_claim| (*non_claim).to_string())
                .collect(),
        }
    }

    #[must_use]
    pub fn projection_claim_level_decision(&self, claim_level: &str) -> ClaimGateDecision {
        if self.verdict == RunnerFidelityVerdict::Failed {
            return ClaimGateDecision::Blocked;
        }

        match claim_level {
            PROJECTION_CLAIM_LEVEL_RAW_OBSERVED | PROJECTION_CLAIM_LEVEL_PROJECTED_EQUIVALENT => {
                self.claim_gate.measured_positive_claims
            }
            PROJECTION_CLAIM_LEVEL_INCONCLUSIVE => ClaimGateDecision::Allowed,
            _ => ClaimGateDecision::Blocked,
        }
    }
}

fn classify_observation_health(
    health: &ObservationHealth,
    reasons: &mut Vec<RunnerFidelityReason>,
) -> RunnerFidelityVerdict {
    if let Err(error) = health.validate() {
        reasons.push(reason_for_health_error(error, health));
        return RunnerFidelityVerdict::Failed;
    }

    if health.platform != "linux" {
        reasons.push(RunnerFidelityReason::new(
            "platform",
            &health.platform,
            "non_linux_platform_has_no_kernel_measurement_surface",
        ));
        return RunnerFidelityVerdict::NotApplicable;
    }

    if health.kernel_layer == KernelLayerStatus::Absent {
        reasons.push(RunnerFidelityReason::new(
            "kernel_layer",
            "absent",
            "absent_kernel_layer_blocks_measured_kernel_effect_claims",
        ));
        return RunnerFidelityVerdict::NotApplicable;
    }

    let is_clipped = if health.ringbuf_drops > 0 {
        reasons.push(RunnerFidelityReason::new(
            "ringbuf_drops",
            health.ringbuf_drops,
            "ringbuf_drops_block_bounded_negative_claims",
        ));
        true
    } else if health.kernel_layer == KernelLayerStatus::PartialRingbufDrops {
        reasons.push(RunnerFidelityReason::new(
            "kernel_layer",
            "partial_ringbuf_drops",
            "partial_kernel_layer_blocks_bounded_negative_claims",
        ));
        true
    } else {
        false
    };

    let is_correlation_partial = health.cgroup_correlation == CgroupCorrelationStatus::Partial;
    if is_correlation_partial {
        reasons.push(RunnerFidelityReason::new(
            "cgroup_correlation",
            "partial",
            "partial_cgroup_correlation_blocks_per_binding_claims",
        ));
    }

    if is_clipped {
        return RunnerFidelityVerdict::Clipped;
    }

    if is_correlation_partial {
        return RunnerFidelityVerdict::CorrelationPartial;
    }

    reasons.push(RunnerFidelityReason::new(
        "observation_health",
        "clean",
        "complete_kernel_layer_zero_drops_clean_cgroup_correlation",
    ));
    RunnerFidelityVerdict::Clean
}

fn reason_for_health_error(
    error: ObservationHealthError,
    health: &ObservationHealth,
) -> RunnerFidelityReason {
    match error {
        ObservationHealthError::InvalidSchema => RunnerFidelityReason::new(
            "schema",
            &health.schema,
            "observation_health_schema_must_match_source_schema",
        ),
        ObservationHealthError::EmptyRunId => {
            RunnerFidelityReason::new("run_id", "", "run_id_required_for_fidelity_verdict")
        }
        ObservationHealthError::RingbufDropsRequirePartialKernelLayer => RunnerFidelityReason::new(
            "ringbuf_drops",
            health.ringbuf_drops,
            "ringbuf_drops_require_partial_kernel_layer",
        ),
        ObservationHealthError::NonLinuxRequiresAbsentKernelLayer => RunnerFidelityReason::new(
            "kernel_layer",
            kernel_layer_label(health.kernel_layer),
            "non_linux_platform_requires_absent_kernel_layer",
        ),
        ObservationHealthError::FailedCgroupCorrelation => RunnerFidelityReason::new(
            "cgroup_correlation",
            "failed",
            "failed_cgroup_correlation_blocks_runner_measured_claims",
        ),
    }
}

fn kernel_layer_label(status: KernelLayerStatus) -> &'static str {
    match status {
        KernelLayerStatus::Complete => "complete",
        KernelLayerStatus::PartialRingbufDrops => "partial_ringbuf_drops",
        KernelLayerStatus::Absent => "absent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PolicyLayerStatus, SdkLayerStatus};

    fn clean_health() -> ObservationHealth {
        let mut health = ObservationHealth::new("run_001", "linux")
            .with_policy_layer(PolicyLayerStatus::Present)
            .with_sdk_layer(SdkLayerStatus::SelfReported)
            .with_cgroup_correlation(CgroupCorrelationStatus::Clean);
        health.kernel_layer = KernelLayerStatus::Complete;
        health
    }

    #[test]
    fn clean_health_allows_measured_and_bounded_negative_claims() {
        let report = RunnerFidelityVerdictReport::from_observation_health(&clean_health());

        assert_eq!(report.schema, RUNNER_FIDELITY_VERDICT_SCHEMA);
        assert_eq!(report.source_schema, OBSERVATION_HEALTH_SCHEMA);
        assert_eq!(report.verdict, RunnerFidelityVerdict::Clean);
        assert_eq!(
            report.claim_gate.measured_positive_claims,
            ClaimGateDecision::Allowed
        );
        assert_eq!(
            report.claim_gate.bounded_negative_claims,
            ClaimGateDecision::Allowed
        );
        assert_eq!(
            report.projection_claim_level_decision(PROJECTION_CLAIM_LEVEL_PROJECTED_EQUIVALENT),
            ClaimGateDecision::Allowed
        );
    }

    #[test]
    fn ringbuf_drops_clip_measurement_and_block_negative_claims() {
        let report = RunnerFidelityVerdictReport::from_observation_health(
            &clean_health().with_ringbuf_drops(4),
        );

        assert_eq!(report.verdict, RunnerFidelityVerdict::Clipped);
        assert_eq!(
            report.claim_gate.measured_positive_claims,
            ClaimGateDecision::Degraded
        );
        assert_eq!(
            report.claim_gate.bounded_negative_claims,
            ClaimGateDecision::Blocked
        );
        assert_eq!(
            report.projection_claim_level_decision(PROJECTION_CLAIM_LEVEL_RAW_OBSERVED),
            ClaimGateDecision::Degraded
        );
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason.rule == "ringbuf_drops_block_bounded_negative_claims"));
    }

    #[test]
    fn partial_cgroup_correlation_blocks_per_binding_claims() {
        let mut health = clean_health();
        health.cgroup_correlation = CgroupCorrelationStatus::Partial;

        let report = RunnerFidelityVerdictReport::from_observation_health(&health);

        assert_eq!(report.verdict, RunnerFidelityVerdict::CorrelationPartial);
        assert_eq!(
            report.claim_gate.measured_positive_claims,
            ClaimGateDecision::Degraded
        );
        assert_eq!(
            report.claim_gate.per_binding_claims,
            ClaimGateDecision::Blocked
        );
    }

    #[test]
    fn clipped_health_with_partial_correlation_preserves_both_degradation_reasons() {
        let mut health = clean_health().with_ringbuf_drops(3);
        health.cgroup_correlation = CgroupCorrelationStatus::Partial;

        let report = RunnerFidelityVerdictReport::from_observation_health(&health);

        assert_eq!(report.verdict, RunnerFidelityVerdict::Clipped);
        assert_eq!(
            report.claim_gate.bounded_negative_claims,
            ClaimGateDecision::Blocked
        );
        assert_eq!(
            report.claim_gate.per_binding_claims,
            ClaimGateDecision::Blocked
        );
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason.rule == "ringbuf_drops_block_bounded_negative_claims"));
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason.rule == "partial_cgroup_correlation_blocks_per_binding_claims"));
    }

    #[test]
    fn non_linux_kernel_measurement_is_not_applicable_but_reported_claims_can_remain() {
        let report = RunnerFidelityVerdictReport::from_observation_health(&ObservationHealth::new(
            "run_macos",
            "macos",
        ));

        assert_eq!(report.verdict, RunnerFidelityVerdict::NotApplicable);
        assert_eq!(
            report.claim_gate.reported_claims,
            ClaimGateDecision::Allowed
        );
        assert_eq!(
            report.claim_gate.measured_positive_claims,
            ClaimGateDecision::Blocked
        );
    }

    #[test]
    fn failed_correlation_blocks_all_claims_from_this_health_record() {
        let mut health = clean_health();
        health.cgroup_correlation = CgroupCorrelationStatus::Failed;

        let report = RunnerFidelityVerdictReport::from_observation_health(&health);

        assert_eq!(report.verdict, RunnerFidelityVerdict::Failed);
        assert_eq!(
            report.claim_gate.reported_claims,
            ClaimGateDecision::Blocked
        );
        assert_eq!(
            report.claim_gate.measured_positive_claims,
            ClaimGateDecision::Blocked
        );
        assert_eq!(
            report.claim_gate.bounded_negative_claims,
            ClaimGateDecision::Blocked
        );
        assert_eq!(
            report.claim_gate.per_binding_claims,
            ClaimGateDecision::Blocked
        );
    }

    #[test]
    fn invalid_non_linux_kernel_reason_uses_contract_vocabulary() {
        let mut health = ObservationHealth::new("run_macos", "macos");
        health.kernel_layer = KernelLayerStatus::Complete;

        let report = RunnerFidelityVerdictReport::from_observation_health(&health);

        assert_eq!(report.verdict, RunnerFidelityVerdict::Failed);
        assert!(report.reasons.iter().any(|reason| {
            reason.field == "kernel_layer"
                && reason.observed == "complete"
                && reason.rule == "non_linux_platform_requires_absent_kernel_layer"
        }));
    }

    #[test]
    fn serialization_uses_runner_verdict_vocabulary() {
        let mut health = clean_health();
        health.cgroup_correlation = CgroupCorrelationStatus::Partial;

        let report = RunnerFidelityVerdictReport::from_observation_health(&health);
        let json = serde_json::to_value(&report).expect("serializes");

        assert_eq!(json["verdict"], "correlation_partial");
        assert_eq!(json["claim_gate"]["per_binding_claims"], "blocked");
    }

    #[test]
    fn unknown_projection_claim_level_is_blocked() {
        let report = RunnerFidelityVerdictReport::from_observation_health(&clean_health());

        assert_eq!(
            report.projection_claim_level_decision("semantic_truth"),
            ClaimGateDecision::Blocked
        );
    }
}
