use super::{reason_codes, Decision, DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath};
use serde::{Deserialize, Serialize};

pub const DENY_PRECEDENCE_VERSION_V1: &str = "wave40_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyClassificationSource {
    OutcomeKind,
    OriginContext,
    FulfillmentPath,
    LegacyDecision,
    NotDeny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenyConvergenceProjection {
    pub policy_deny: bool,
    pub fail_closed_deny: bool,
    pub enforcement_deny: bool,
    pub classification_source: DenyClassificationSource,
    pub legacy_fallback_applied: bool,
    pub deny_convergence_reason: &'static str,
}

pub fn project_deny_convergence(
    decision_outcome_kind: Option<DecisionOutcomeKind>,
    decision_origin: Option<DecisionOrigin>,
    fulfillment_decision_path: Option<FulfillmentDecisionPath>,
    decision: Decision,
    fail_closed_applied: bool,
    reason_code: &str,
) -> DenyConvergenceProjection {
    if let Some(kind) = decision_outcome_kind {
        return match kind {
            DecisionOutcomeKind::PolicyDeny => projection(
                true,
                false,
                false,
                DenyClassificationSource::OutcomeKind,
                false,
                "outcome_policy_deny",
            ),
            DecisionOutcomeKind::FailClosedDeny => projection(
                false,
                true,
                false,
                DenyClassificationSource::OutcomeKind,
                false,
                "outcome_fail_closed_deny",
            ),
            DecisionOutcomeKind::EnforcementDeny => projection(
                false,
                false,
                true,
                DenyClassificationSource::OutcomeKind,
                false,
                "outcome_enforcement_deny",
            ),
            DecisionOutcomeKind::ObligationApplied
            | DecisionOutcomeKind::ObligationSkipped
            | DecisionOutcomeKind::ObligationError => not_deny_projection(
                DenyClassificationSource::OutcomeKind,
                false,
                "outcome_not_deny",
            ),
        };
    }

    if let Some(origin_projection) =
        project_from_origin(decision_origin, decision, fail_closed_applied, reason_code)
    {
        return origin_projection;
    }

    if let Some(path_projection) = project_from_fulfillment_path(fulfillment_decision_path) {
        return path_projection;
    }

    project_from_legacy_decision(decision, fail_closed_applied, reason_code)
}

fn projection(
    policy_deny: bool,
    fail_closed_deny: bool,
    enforcement_deny: bool,
    classification_source: DenyClassificationSource,
    legacy_fallback_applied: bool,
    deny_convergence_reason: &'static str,
) -> DenyConvergenceProjection {
    DenyConvergenceProjection {
        policy_deny,
        fail_closed_deny,
        enforcement_deny,
        classification_source,
        legacy_fallback_applied,
        deny_convergence_reason,
    }
}

fn not_deny_projection(
    source: DenyClassificationSource,
    fallback: bool,
    reason: &'static str,
) -> DenyConvergenceProjection {
    projection(false, false, false, source, fallback, reason)
}

fn project_from_origin(
    decision_origin: Option<DecisionOrigin>,
    decision: Decision,
    fail_closed_applied: bool,
    reason_code: &str,
) -> Option<DenyConvergenceProjection> {
    let origin = decision_origin?;
    match origin {
        DecisionOrigin::FailClosedMatrix => Some(projection(
            false,
            true,
            false,
            DenyClassificationSource::OriginContext,
            true,
            "origin_fail_closed_matrix",
        )),
        DecisionOrigin::RuntimeEnforcement => Some(projection(
            false,
            false,
            true,
            DenyClassificationSource::OriginContext,
            true,
            "origin_runtime_enforcement",
        )),
        DecisionOrigin::PolicyEngine => match decision {
            Decision::Deny => Some(projection(
                true,
                false,
                false,
                DenyClassificationSource::OriginContext,
                true,
                "origin_policy_engine_deny",
            )),
            Decision::Error => Some(projection(
                false,
                false,
                true,
                DenyClassificationSource::OriginContext,
                true,
                "origin_policy_engine_error",
            )),
            Decision::Allow => Some(not_deny_projection(
                DenyClassificationSource::OriginContext,
                true,
                "origin_policy_engine_allow",
            )),
        },
        DecisionOrigin::ObligationExecutor => Some(project_from_legacy_decision(
            decision,
            fail_closed_applied,
            reason_code,
        )),
    }
}

fn project_from_fulfillment_path(
    fulfillment_decision_path: Option<FulfillmentDecisionPath>,
) -> Option<DenyConvergenceProjection> {
    let path = fulfillment_decision_path?;
    match path {
        FulfillmentDecisionPath::PolicyDeny => Some(projection(
            true,
            false,
            false,
            DenyClassificationSource::FulfillmentPath,
            true,
            "fulfillment_policy_deny",
        )),
        FulfillmentDecisionPath::FailClosedDeny => Some(projection(
            false,
            true,
            false,
            DenyClassificationSource::FulfillmentPath,
            true,
            "fulfillment_fail_closed_deny",
        )),
        FulfillmentDecisionPath::DecisionError => Some(projection(
            false,
            false,
            true,
            DenyClassificationSource::FulfillmentPath,
            true,
            "fulfillment_decision_error",
        )),
        FulfillmentDecisionPath::PolicyAllow => Some(not_deny_projection(
            DenyClassificationSource::FulfillmentPath,
            true,
            "fulfillment_policy_allow",
        )),
    }
}

fn project_from_legacy_decision(
    decision: Decision,
    fail_closed_applied: bool,
    reason_code: &str,
) -> DenyConvergenceProjection {
    match decision {
        Decision::Deny => {
            if fail_closed_applied {
                projection(
                    false,
                    true,
                    false,
                    DenyClassificationSource::LegacyDecision,
                    true,
                    "legacy_fail_closed_deny",
                )
            } else if is_enforcement_deny_reason(reason_code) {
                projection(
                    false,
                    false,
                    true,
                    DenyClassificationSource::LegacyDecision,
                    true,
                    "legacy_enforcement_deny",
                )
            } else {
                projection(
                    true,
                    false,
                    false,
                    DenyClassificationSource::LegacyDecision,
                    true,
                    "legacy_policy_deny",
                )
            }
        }
        Decision::Error => projection(
            false,
            false,
            true,
            DenyClassificationSource::LegacyDecision,
            true,
            "legacy_decision_error",
        ),
        Decision::Allow => not_deny_projection(
            DenyClassificationSource::NotDeny,
            true,
            "legacy_decision_allow",
        ),
    }
}

fn is_enforcement_deny_reason(reason_code: &str) -> bool {
    reason_code.starts_with("M_")
        || matches!(
            reason_code,
            reason_codes::P_APPROVAL_REQUIRED
                | reason_codes::P_RESTRICT_SCOPE
                | reason_codes::P_REDACT_ARGS
                | reason_codes::P_MANDATE_REQUIRED
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_outcome_kind_for_policy_deny() {
        let projection = project_deny_convergence(
            Some(DecisionOutcomeKind::PolicyDeny),
            Some(DecisionOrigin::PolicyEngine),
            Some(FulfillmentDecisionPath::PolicyDeny),
            Decision::Deny,
            false,
            reason_codes::P_POLICY_DENY,
        );
        assert!(projection.policy_deny);
        assert!(!projection.fail_closed_deny);
        assert!(!projection.enforcement_deny);
        assert_eq!(
            projection.classification_source,
            DenyClassificationSource::OutcomeKind
        );
        assert!(!projection.legacy_fallback_applied);
        assert_eq!(projection.deny_convergence_reason, "outcome_policy_deny");
    }

    #[test]
    fn falls_back_to_origin_context() {
        let projection = project_deny_convergence(
            None,
            Some(DecisionOrigin::FailClosedMatrix),
            Some(FulfillmentDecisionPath::PolicyDeny),
            Decision::Deny,
            true,
            reason_codes::S_DB_ERROR,
        );
        assert!(!projection.policy_deny);
        assert!(projection.fail_closed_deny);
        assert!(!projection.enforcement_deny);
        assert_eq!(
            projection.classification_source,
            DenyClassificationSource::OriginContext
        );
        assert!(projection.legacy_fallback_applied);
        assert_eq!(
            projection.deny_convergence_reason,
            "origin_fail_closed_matrix"
        );
    }

    #[test]
    fn falls_back_to_fulfillment_path() {
        let projection = project_deny_convergence(
            None,
            None,
            Some(FulfillmentDecisionPath::DecisionError),
            Decision::Error,
            false,
            reason_codes::S_INTERNAL_ERROR,
        );
        assert!(!projection.policy_deny);
        assert!(!projection.fail_closed_deny);
        assert!(projection.enforcement_deny);
        assert_eq!(
            projection.classification_source,
            DenyClassificationSource::FulfillmentPath
        );
        assert!(projection.legacy_fallback_applied);
        assert_eq!(
            projection.deny_convergence_reason,
            "fulfillment_decision_error"
        );
    }

    #[test]
    fn falls_back_to_legacy_decision() {
        let projection = project_deny_convergence(
            None,
            None,
            None,
            Decision::Deny,
            false,
            reason_codes::P_POLICY_DENY,
        );
        assert!(projection.policy_deny);
        assert!(!projection.fail_closed_deny);
        assert!(!projection.enforcement_deny);
        assert_eq!(
            projection.classification_source,
            DenyClassificationSource::LegacyDecision
        );
        assert!(projection.legacy_fallback_applied);
        assert_eq!(projection.deny_convergence_reason, "legacy_policy_deny");
    }
}
