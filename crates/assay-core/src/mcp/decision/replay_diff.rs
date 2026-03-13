use super::{
    DecisionData, DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath, OutcomeCompatState,
};
use crate::mcp::policy::TypedPolicyDecision;
use serde::{Deserialize, Serialize};

/// Canonical replay-diff bucket for deterministic policy comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayDiffBucket {
    Unchanged,
    Stricter,
    Looser,
    Reclassified,
    EvidenceOnly,
}

/// Frozen replay basis used for deterministic diffing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayDiffBasis {
    pub decision_outcome_kind: Option<DecisionOutcomeKind>,
    pub decision_origin: Option<DecisionOrigin>,
    pub outcome_compat_state: Option<OutcomeCompatState>,
    pub fulfillment_decision_path: Option<FulfillmentDecisionPath>,
    pub reason_code: String,
    pub typed_decision: Option<TypedPolicyDecision>,
    pub policy_version: Option<String>,
    pub policy_digest: Option<String>,
}

/// Build replay basis from an emitted decision payload.
pub fn basis_from_decision_data(data: &DecisionData) -> ReplayDiffBasis {
    ReplayDiffBasis {
        decision_outcome_kind: data.decision_outcome_kind,
        decision_origin: data.decision_origin,
        outcome_compat_state: data.outcome_compat_state,
        fulfillment_decision_path: data.fulfillment_decision_path,
        reason_code: data.reason_code.clone(),
        typed_decision: data.typed_decision,
        policy_version: data.policy_version.clone(),
        policy_digest: data.policy_digest.clone(),
    }
}

/// Classify replay diff between baseline and candidate basis.
pub fn classify_replay_diff(
    baseline: &ReplayDiffBasis,
    candidate: &ReplayDiffBasis,
) -> ReplayDiffBucket {
    if baseline == candidate {
        return ReplayDiffBucket::Unchanged;
    }

    if same_effective_decision_class(baseline, candidate) {
        return ReplayDiffBucket::EvidenceOnly;
    }

    let baseline_rank = restrictiveness_rank(baseline);
    let candidate_rank = restrictiveness_rank(candidate);

    if candidate_rank > baseline_rank {
        return ReplayDiffBucket::Stricter;
    }

    if candidate_rank < baseline_rank {
        return ReplayDiffBucket::Looser;
    }

    ReplayDiffBucket::Reclassified
}

fn same_effective_decision_class(baseline: &ReplayDiffBasis, candidate: &ReplayDiffBasis) -> bool {
    baseline.decision_outcome_kind == candidate.decision_outcome_kind
        && baseline.decision_origin == candidate.decision_origin
        && baseline.outcome_compat_state == candidate.outcome_compat_state
        && baseline.fulfillment_decision_path == candidate.fulfillment_decision_path
        && baseline.reason_code == candidate.reason_code
        && baseline.typed_decision == candidate.typed_decision
}

fn restrictiveness_rank(basis: &ReplayDiffBasis) -> u8 {
    match basis.decision_outcome_kind {
        Some(DecisionOutcomeKind::PolicyDeny)
        | Some(DecisionOutcomeKind::FailClosedDeny)
        | Some(DecisionOutcomeKind::EnforcementDeny) => 2,
        Some(DecisionOutcomeKind::ObligationApplied)
        | Some(DecisionOutcomeKind::ObligationSkipped)
        | Some(DecisionOutcomeKind::ObligationError) => 1,
        None => match basis.fulfillment_decision_path {
            Some(FulfillmentDecisionPath::PolicyDeny)
            | Some(FulfillmentDecisionPath::FailClosedDeny)
            | Some(FulfillmentDecisionPath::DecisionError) => 2,
            Some(FulfillmentDecisionPath::PolicyAllow) | None => 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_basis(kind: Option<DecisionOutcomeKind>, reason: &str) -> ReplayDiffBasis {
        ReplayDiffBasis {
            decision_outcome_kind: kind,
            decision_origin: Some(DecisionOrigin::PolicyEngine),
            outcome_compat_state: Some(OutcomeCompatState::LegacyFieldsPreserved),
            fulfillment_decision_path: Some(FulfillmentDecisionPath::PolicyAllow),
            reason_code: reason.to_string(),
            typed_decision: Some(TypedPolicyDecision::AllowWithObligations),
            policy_version: Some("v1".to_string()),
            policy_digest: Some("sha1".to_string()),
        }
    }

    #[test]
    fn classifies_unchanged() {
        let a = make_basis(
            Some(DecisionOutcomeKind::ObligationApplied),
            "P_POLICY_ALLOW",
        );
        assert_eq!(classify_replay_diff(&a, &a), ReplayDiffBucket::Unchanged);
    }

    #[test]
    fn classifies_evidence_only() {
        let baseline = make_basis(
            Some(DecisionOutcomeKind::ObligationApplied),
            "P_POLICY_ALLOW",
        );
        let mut candidate = baseline.clone();
        candidate.policy_version = Some("v2".to_string());
        candidate.policy_digest = Some("sha2".to_string());
        assert_eq!(
            classify_replay_diff(&baseline, &candidate),
            ReplayDiffBucket::EvidenceOnly
        );
    }

    #[test]
    fn classifies_stricter_and_looser() {
        let allow = make_basis(
            Some(DecisionOutcomeKind::ObligationApplied),
            "P_POLICY_ALLOW",
        );
        let deny = make_basis(Some(DecisionOutcomeKind::PolicyDeny), "P_POLICY_DENY");
        assert_eq!(
            classify_replay_diff(&allow, &deny),
            ReplayDiffBucket::Stricter
        );
        assert_eq!(
            classify_replay_diff(&deny, &allow),
            ReplayDiffBucket::Looser
        );
    }

    #[test]
    fn classifies_reclassified() {
        let mut baseline = make_basis(Some(DecisionOutcomeKind::PolicyDeny), "P_POLICY_DENY");
        baseline.fulfillment_decision_path = Some(FulfillmentDecisionPath::PolicyDeny);

        let mut candidate = baseline.clone();
        candidate.decision_outcome_kind = Some(DecisionOutcomeKind::FailClosedDeny);
        candidate.decision_origin = Some(DecisionOrigin::FailClosedMatrix);
        candidate.fulfillment_decision_path = Some(FulfillmentDecisionPath::FailClosedDeny);

        assert_eq!(
            classify_replay_diff(&baseline, &candidate),
            ReplayDiffBucket::Reclassified
        );
    }
}
