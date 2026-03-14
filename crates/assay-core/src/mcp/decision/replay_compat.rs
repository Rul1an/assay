use super::{
    Decision, DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath, OutcomeCompatState,
};
use serde::{Deserialize, Serialize};

pub const DECISION_BASIS_VERSION_V1: &str = "wave39_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClassificationSource {
    ConvergedOutcome,
    FulfillmentPath,
    LegacyFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayCompatProjection {
    pub compat_fallback_applied: bool,
    pub classification_source: ReplayClassificationSource,
    pub replay_diff_reason: &'static str,
    pub legacy_shape_detected: bool,
}

pub fn project_replay_compat(
    decision_outcome_kind: Option<DecisionOutcomeKind>,
    decision_origin: Option<DecisionOrigin>,
    outcome_compat_state: Option<OutcomeCompatState>,
    fulfillment_decision_path: Option<FulfillmentDecisionPath>,
    decision: Decision,
) -> ReplayCompatProjection {
    let legacy_shape_detected = decision_outcome_kind.is_none()
        || decision_origin.is_none()
        || outcome_compat_state.is_none()
        || fulfillment_decision_path.is_none();

    let (classification_source, replay_diff_reason) = if let Some(kind) = decision_outcome_kind {
        (
            ReplayClassificationSource::ConvergedOutcome,
            reason_from_outcome_kind(kind),
        )
    } else if let Some(path) = fulfillment_decision_path {
        (
            ReplayClassificationSource::FulfillmentPath,
            reason_from_fulfillment_path(path),
        )
    } else {
        (
            ReplayClassificationSource::LegacyFallback,
            reason_from_legacy_decision(decision),
        )
    };

    ReplayCompatProjection {
        compat_fallback_applied: classification_source
            != ReplayClassificationSource::ConvergedOutcome,
        classification_source,
        replay_diff_reason,
        legacy_shape_detected,
    }
}

fn reason_from_outcome_kind(kind: DecisionOutcomeKind) -> &'static str {
    match kind {
        DecisionOutcomeKind::PolicyDeny => "converged_policy_deny",
        DecisionOutcomeKind::FailClosedDeny => "converged_fail_closed_deny",
        DecisionOutcomeKind::ObligationApplied => "converged_obligation_applied",
        DecisionOutcomeKind::ObligationSkipped => "converged_obligation_skipped",
        DecisionOutcomeKind::ObligationError => "converged_obligation_error",
        DecisionOutcomeKind::EnforcementDeny => "converged_enforcement_deny",
    }
}

fn reason_from_fulfillment_path(path: FulfillmentDecisionPath) -> &'static str {
    match path {
        FulfillmentDecisionPath::PolicyAllow => "fulfillment_policy_allow",
        FulfillmentDecisionPath::PolicyDeny => "fulfillment_policy_deny",
        FulfillmentDecisionPath::FailClosedDeny => "fulfillment_fail_closed_deny",
        FulfillmentDecisionPath::DecisionError => "fulfillment_decision_error",
    }
}

fn reason_from_legacy_decision(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "legacy_decision_allow",
        Decision::Deny => "legacy_decision_deny",
        Decision::Error => "legacy_decision_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_prefers_converged_markers() {
        let projection = project_replay_compat(
            Some(DecisionOutcomeKind::ObligationApplied),
            Some(DecisionOrigin::ObligationExecutor),
            Some(OutcomeCompatState::LegacyFieldsPreserved),
            Some(FulfillmentDecisionPath::PolicyAllow),
            Decision::Allow,
        );

        assert_eq!(
            projection.classification_source,
            ReplayClassificationSource::ConvergedOutcome
        );
        assert_eq!(
            projection.replay_diff_reason,
            "converged_obligation_applied"
        );
        assert!(!projection.compat_fallback_applied);
        assert!(!projection.legacy_shape_detected);
    }

    #[test]
    fn precedence_falls_back_to_fulfillment_path() {
        let projection = project_replay_compat(
            None,
            None,
            None,
            Some(FulfillmentDecisionPath::PolicyAllow),
            Decision::Allow,
        );

        assert_eq!(
            projection.classification_source,
            ReplayClassificationSource::FulfillmentPath
        );
        assert_eq!(projection.replay_diff_reason, "fulfillment_policy_allow");
        assert!(projection.compat_fallback_applied);
        assert!(projection.legacy_shape_detected);
    }

    #[test]
    fn precedence_falls_back_to_legacy_decision() {
        let projection = project_replay_compat(None, None, None, None, Decision::Deny);

        assert_eq!(
            projection.classification_source,
            ReplayClassificationSource::LegacyFallback
        );
        assert_eq!(projection.replay_diff_reason, "legacy_decision_deny");
        assert!(projection.compat_fallback_applied);
        assert!(projection.legacy_shape_detected);
    }
}
