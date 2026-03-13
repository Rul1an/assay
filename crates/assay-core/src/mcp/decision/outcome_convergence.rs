use super::{reason_codes, Decision};
use serde::{Deserialize, Serialize};

/// Canonical decision/evidence convergence outcome classification (Wave37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcomeKind {
    PolicyDeny,
    FailClosedDeny,
    EnforcementDeny,
    ObligationApplied,
    ObligationSkipped,
    ObligationError,
}

/// Origin for a canonical convergence outcome classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOrigin {
    PolicyEngine,
    FailClosedMatrix,
    RuntimeEnforcement,
    ObligationExecutor,
}

/// Compatibility state for downstream event consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeCompatState {
    LegacyFieldsPreserved,
}

pub(super) struct OutcomeClassification {
    pub(super) kind: DecisionOutcomeKind,
    pub(super) origin: DecisionOrigin,
    pub(super) compat_state: OutcomeCompatState,
}

pub(super) fn classify_decision_outcome(
    decision: Decision,
    reason_code: &str,
    fail_closed_applied: bool,
    obligation_applied_present: bool,
    _obligation_skipped_present: bool,
    obligation_error_present: bool,
) -> OutcomeClassification {
    let (kind, origin) = match decision {
        Decision::Deny => {
            if fail_closed_applied {
                (
                    DecisionOutcomeKind::FailClosedDeny,
                    DecisionOrigin::FailClosedMatrix,
                )
            } else if is_enforcement_deny_reason(reason_code) {
                (
                    DecisionOutcomeKind::EnforcementDeny,
                    DecisionOrigin::RuntimeEnforcement,
                )
            } else {
                (
                    DecisionOutcomeKind::PolicyDeny,
                    DecisionOrigin::PolicyEngine,
                )
            }
        }
        Decision::Allow => {
            if obligation_error_present {
                (
                    DecisionOutcomeKind::ObligationError,
                    DecisionOrigin::ObligationExecutor,
                )
            } else if obligation_applied_present {
                (
                    DecisionOutcomeKind::ObligationApplied,
                    DecisionOrigin::ObligationExecutor,
                )
            } else {
                (
                    DecisionOutcomeKind::ObligationSkipped,
                    DecisionOrigin::ObligationExecutor,
                )
            }
        }
        Decision::Error => (
            DecisionOutcomeKind::EnforcementDeny,
            DecisionOrigin::RuntimeEnforcement,
        ),
    };

    OutcomeClassification {
        kind,
        origin,
        compat_state: OutcomeCompatState::LegacyFieldsPreserved,
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
    fn classifies_policy_deny() {
        let result = classify_decision_outcome(
            Decision::Deny,
            reason_codes::P_TOOL_DENIED,
            false,
            false,
            false,
            false,
        );
        assert_eq!(result.kind, DecisionOutcomeKind::PolicyDeny);
        assert_eq!(result.origin, DecisionOrigin::PolicyEngine);
        assert_eq!(
            result.compat_state,
            OutcomeCompatState::LegacyFieldsPreserved
        );
    }

    #[test]
    fn classifies_fail_closed_deny() {
        let result = classify_decision_outcome(
            Decision::Deny,
            reason_codes::S_DB_ERROR,
            true,
            false,
            false,
            false,
        );
        assert_eq!(result.kind, DecisionOutcomeKind::FailClosedDeny);
        assert_eq!(result.origin, DecisionOrigin::FailClosedMatrix);
    }

    #[test]
    fn classifies_enforcement_deny() {
        let result = classify_decision_outcome(
            Decision::Deny,
            reason_codes::P_APPROVAL_REQUIRED,
            false,
            false,
            false,
            true,
        );
        assert_eq!(result.kind, DecisionOutcomeKind::EnforcementDeny);
        assert_eq!(result.origin, DecisionOrigin::RuntimeEnforcement);
    }

    #[test]
    fn classifies_mandate_deny_as_enforcement_deny() {
        let result = classify_decision_outcome(
            Decision::Deny,
            reason_codes::M_EXPIRED,
            false,
            false,
            false,
            false,
        );
        assert_eq!(result.kind, DecisionOutcomeKind::EnforcementDeny);
        assert_eq!(result.origin, DecisionOrigin::RuntimeEnforcement);
    }

    #[test]
    fn classifies_decision_error_as_enforcement_deny() {
        let result = classify_decision_outcome(
            Decision::Error,
            reason_codes::S_INTERNAL_ERROR,
            false,
            false,
            false,
            false,
        );
        assert_eq!(result.kind, DecisionOutcomeKind::EnforcementDeny);
        assert_eq!(result.origin, DecisionOrigin::RuntimeEnforcement);
    }

    #[test]
    fn classifies_obligation_outcomes_on_allow() {
        let applied = classify_decision_outcome(
            Decision::Allow,
            reason_codes::P_POLICY_ALLOW,
            false,
            true,
            false,
            false,
        );
        assert_eq!(applied.kind, DecisionOutcomeKind::ObligationApplied);

        let skipped = classify_decision_outcome(
            Decision::Allow,
            reason_codes::P_POLICY_ALLOW,
            false,
            false,
            true,
            false,
        );
        assert_eq!(skipped.kind, DecisionOutcomeKind::ObligationSkipped);

        let errored = classify_decision_outcome(
            Decision::Allow,
            reason_codes::P_POLICY_ALLOW,
            false,
            false,
            false,
            true,
        );
        assert_eq!(errored.kind, DecisionOutcomeKind::ObligationError);
    }
}
