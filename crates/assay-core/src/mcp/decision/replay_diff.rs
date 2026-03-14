use super::{
    deny_convergence::{project_deny_convergence, DENY_PRECEDENCE_VERSION_V1},
    replay_compat::{project_replay_compat, DECISION_BASIS_VERSION_V1},
    Decision, DecisionData, DecisionOrigin, DecisionOutcomeKind, DenyClassificationSource,
    FulfillmentDecisionPath, OutcomeCompatState, ReplayClassificationSource,
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
    pub decision_basis_version: String,
    pub compat_fallback_applied: bool,
    pub classification_source: ReplayClassificationSource,
    pub replay_diff_reason: String,
    pub legacy_shape_detected: bool,
    pub policy_deny: bool,
    pub fail_closed_deny: bool,
    pub enforcement_deny: bool,
    pub deny_precedence_version: String,
    pub deny_classification_source: DenyClassificationSource,
    pub deny_legacy_fallback_applied: bool,
    pub deny_convergence_reason: String,
    pub reason_code: String,
    pub typed_decision: Option<TypedPolicyDecision>,
    pub policy_version: Option<String>,
    pub policy_digest: Option<String>,
    pub decision: Decision,
    pub fail_closed_applied: bool,
}

/// Build replay basis from an emitted decision payload.
pub fn basis_from_decision_data(data: &DecisionData) -> ReplayDiffBasis {
    let fail_closed_applied = data
        .fail_closed
        .as_ref()
        .map(|ctx| ctx.fail_closed_applied)
        .unwrap_or(false);

    let replay_projection = project_replay_compat(
        data.decision_outcome_kind,
        data.decision_origin,
        data.outcome_compat_state,
        data.fulfillment_decision_path,
        data.decision,
    );
    let deny_projection = project_deny_convergence(
        data.decision_outcome_kind,
        data.decision_origin,
        data.fulfillment_decision_path,
        data.decision,
        fail_closed_applied,
        data.reason_code.as_str(),
    );

    ReplayDiffBasis {
        decision_outcome_kind: data.decision_outcome_kind,
        decision_origin: data.decision_origin,
        outcome_compat_state: data.outcome_compat_state,
        fulfillment_decision_path: data.fulfillment_decision_path,
        decision_basis_version: data
            .decision_basis_version
            .clone()
            .unwrap_or_else(|| DECISION_BASIS_VERSION_V1.to_string()),
        compat_fallback_applied: data
            .compat_fallback_applied
            .unwrap_or(replay_projection.compat_fallback_applied),
        classification_source: data
            .classification_source
            .unwrap_or(replay_projection.classification_source),
        replay_diff_reason: data
            .replay_diff_reason
            .clone()
            .unwrap_or_else(|| replay_projection.replay_diff_reason.to_string()),
        legacy_shape_detected: data
            .legacy_shape_detected
            .unwrap_or(replay_projection.legacy_shape_detected),
        policy_deny: data.policy_deny.unwrap_or(deny_projection.policy_deny),
        fail_closed_deny: data
            .fail_closed_deny
            .unwrap_or(deny_projection.fail_closed_deny),
        enforcement_deny: data
            .enforcement_deny
            .unwrap_or(deny_projection.enforcement_deny),
        deny_precedence_version: data
            .deny_precedence_version
            .clone()
            .unwrap_or_else(|| DENY_PRECEDENCE_VERSION_V1.to_string()),
        deny_classification_source: data
            .deny_classification_source
            .unwrap_or(deny_projection.classification_source),
        deny_legacy_fallback_applied: data
            .deny_legacy_fallback_applied
            .unwrap_or(deny_projection.legacy_fallback_applied),
        deny_convergence_reason: data
            .deny_convergence_reason
            .clone()
            .unwrap_or_else(|| deny_projection.deny_convergence_reason.to_string()),
        reason_code: data.reason_code.clone(),
        typed_decision: data.typed_decision,
        policy_version: data.policy_version.clone(),
        policy_digest: data.policy_digest.clone(),
        decision: data.decision,
        fail_closed_applied,
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
        && baseline.decision_basis_version == candidate.decision_basis_version
        && baseline.compat_fallback_applied == candidate.compat_fallback_applied
        && baseline.classification_source == candidate.classification_source
        && baseline.replay_diff_reason == candidate.replay_diff_reason
        && baseline.legacy_shape_detected == candidate.legacy_shape_detected
        && baseline.policy_deny == candidate.policy_deny
        && baseline.fail_closed_deny == candidate.fail_closed_deny
        && baseline.enforcement_deny == candidate.enforcement_deny
        && baseline.deny_precedence_version == candidate.deny_precedence_version
        && baseline.deny_classification_source == candidate.deny_classification_source
        && baseline.deny_legacy_fallback_applied == candidate.deny_legacy_fallback_applied
        && baseline.deny_convergence_reason == candidate.deny_convergence_reason
        && baseline.reason_code == candidate.reason_code
        && baseline.typed_decision == candidate.typed_decision
        && baseline.decision == candidate.decision
        && baseline.fail_closed_applied == candidate.fail_closed_applied
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
            Some(FulfillmentDecisionPath::PolicyAllow) => 1,
            None => match basis.decision {
                Decision::Deny | Decision::Error => 2,
                Decision::Allow => 1,
            },
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
            decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
            compat_fallback_applied: false,
            classification_source: ReplayClassificationSource::ConvergedOutcome,
            replay_diff_reason: "converged_obligation_applied".to_string(),
            legacy_shape_detected: false,
            policy_deny: false,
            fail_closed_deny: false,
            enforcement_deny: false,
            deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
            deny_classification_source: DenyClassificationSource::NotDeny,
            deny_legacy_fallback_applied: false,
            deny_convergence_reason: "outcome_not_deny".to_string(),
            reason_code: reason.to_string(),
            typed_decision: Some(TypedPolicyDecision::AllowWithObligations),
            policy_version: Some("v1".to_string()),
            policy_digest: Some("sha1".to_string()),
            decision: Decision::Allow,
            fail_closed_applied: false,
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
        baseline.decision = Decision::Deny;

        let mut candidate = baseline.clone();
        candidate.decision_outcome_kind = Some(DecisionOutcomeKind::FailClosedDeny);
        candidate.decision_origin = Some(DecisionOrigin::FailClosedMatrix);
        candidate.fulfillment_decision_path = Some(FulfillmentDecisionPath::FailClosedDeny);
        candidate.fail_closed_applied = true;

        assert_eq!(
            classify_replay_diff(&baseline, &candidate),
            ReplayDiffBucket::Reclassified
        );
    }

    #[test]
    fn classifies_legacy_events_with_decision_fallback() {
        let baseline = ReplayDiffBasis {
            decision_outcome_kind: None,
            decision_origin: None,
            outcome_compat_state: None,
            fulfillment_decision_path: None,
            decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
            compat_fallback_applied: true,
            classification_source: ReplayClassificationSource::LegacyFallback,
            replay_diff_reason: "legacy_decision_allow".to_string(),
            legacy_shape_detected: true,
            policy_deny: false,
            fail_closed_deny: false,
            enforcement_deny: false,
            deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
            deny_classification_source: DenyClassificationSource::NotDeny,
            deny_legacy_fallback_applied: true,
            deny_convergence_reason: "legacy_decision_allow".to_string(),
            reason_code: "P_POLICY_ALLOW".to_string(),
            typed_decision: None,
            policy_version: None,
            policy_digest: None,
            decision: Decision::Allow,
            fail_closed_applied: false,
        };
        let candidate = ReplayDiffBasis {
            decision: Decision::Deny,
            reason_code: "P_POLICY_DENY".to_string(),
            ..baseline.clone()
        };

        assert_eq!(
            classify_replay_diff(&baseline, &candidate),
            ReplayDiffBucket::Stricter
        );
        assert_eq!(
            classify_replay_diff(&candidate, &baseline),
            ReplayDiffBucket::Looser
        );
    }
}
