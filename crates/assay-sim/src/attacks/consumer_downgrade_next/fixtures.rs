use assay_core::mcp::decision::{
    required_consumer_fields_v1, ConsumerPayloadState, ConsumerReadPath, Decision, DecisionOrigin,
    DecisionOutcomeKind, DenyClassificationSource, FulfillmentDecisionPath, OutcomeCompatState,
    ReplayClassificationSource, ReplayDiffBasis, DECISION_BASIS_VERSION_V1,
    DECISION_CONSUMER_CONTRACT_VERSION_V1, DENY_PRECEDENCE_VERSION_V1,
};

pub(super) fn make_converged_deny_basis() -> ReplayDiffBasis {
    ReplayDiffBasis {
        decision_outcome_kind: Some(DecisionOutcomeKind::PolicyDeny),
        decision_origin: Some(DecisionOrigin::PolicyEngine),
        outcome_compat_state: Some(OutcomeCompatState::LegacyFieldsPreserved),
        fulfillment_decision_path: Some(FulfillmentDecisionPath::PolicyDeny),
        decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
        compat_fallback_applied: false,
        classification_source: ReplayClassificationSource::ConvergedOutcome,
        replay_diff_reason: "converged_policy_deny".to_string(),
        legacy_shape_detected: false,
        decision_consumer_contract_version: DECISION_CONSUMER_CONTRACT_VERSION_V1.to_string(),
        consumer_read_path: ConsumerReadPath::ConvergedDecision,
        consumer_fallback_applied: false,
        consumer_payload_state: ConsumerPayloadState::Converged,
        required_consumer_fields: required_consumer_fields_v1(),
        policy_deny: true,
        fail_closed_deny: false,
        enforcement_deny: false,
        deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
        deny_classification_source: DenyClassificationSource::OutcomeKind,
        deny_legacy_fallback_applied: false,
        deny_convergence_reason: "outcome_policy_deny".to_string(),
        reason_code: "policy_deny_sensitive_tool".to_string(),
        typed_decision: None,
        policy_version: Some("v1".to_string()),
        policy_digest: Some("sha256:abc".to_string()),
        decision: Decision::Allow, // legacy field diverges from converged
        fail_closed_applied: false,
    }
}

pub(super) fn make_converged_allow_basis() -> ReplayDiffBasis {
    ReplayDiffBasis {
        decision_outcome_kind: Some(DecisionOutcomeKind::ObligationApplied),
        decision_origin: Some(DecisionOrigin::PolicyEngine),
        outcome_compat_state: Some(OutcomeCompatState::LegacyFieldsPreserved),
        fulfillment_decision_path: Some(FulfillmentDecisionPath::PolicyAllow),
        decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
        compat_fallback_applied: false,
        classification_source: ReplayClassificationSource::ConvergedOutcome,
        replay_diff_reason: "converged_obligation_applied".to_string(),
        legacy_shape_detected: false,
        decision_consumer_contract_version: DECISION_CONSUMER_CONTRACT_VERSION_V1.to_string(),
        consumer_read_path: ConsumerReadPath::ConvergedDecision,
        consumer_fallback_applied: false,
        consumer_payload_state: ConsumerPayloadState::Converged,
        required_consumer_fields: required_consumer_fields_v1(),
        policy_deny: false,
        fail_closed_deny: false,
        enforcement_deny: false,
        deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
        deny_classification_source: DenyClassificationSource::OutcomeKind,
        deny_legacy_fallback_applied: false,
        deny_convergence_reason: "outcome_not_deny".to_string(),
        reason_code: "obligation_applied_log".to_string(),
        typed_decision: None,
        policy_version: Some("v1".to_string()),
        policy_digest: Some("sha256:abc".to_string()),
        decision: Decision::Allow,
        fail_closed_applied: false,
    }
}

/// Canonical restrictiveness: deny variants = 2, allow/obligation = 1.
pub(super) fn canonical_rank(basis: &ReplayDiffBasis) -> u8 {
    match basis.decision_outcome_kind {
        Some(DecisionOutcomeKind::PolicyDeny)
        | Some(DecisionOutcomeKind::FailClosedDeny)
        | Some(DecisionOutcomeKind::EnforcementDeny) => 2,
        _ => 1,
    }
}

/// Partial consumer rank: reads only legacy `decision` field.
pub(super) fn legacy_only_rank(basis: &ReplayDiffBasis) -> u8 {
    match basis.decision {
        Decision::Deny | Decision::Error => 2,
        Decision::Allow => 1,
    }
}
