use assay_core::mcp::decision::{
    reason_codes, DecisionEvent, DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath,
    ObligationOutcome, ObligationOutcomeStatus, OutcomeCompatState, PolicyDecisionEventContext,
};
use assay_core::mcp::policy::{
    FailClosedContext, FailClosedMode, FailClosedTrigger, ToolRiskClass,
};

#[test]
fn fulfillment_normalizes_outcomes_and_sets_policy_deny_path() {
    let context = PolicyDecisionEventContext {
        obligation_outcomes: vec![ObligationOutcome {
            obligation_type: "restrict_scope".to_string(),
            status: ObligationOutcomeStatus::Applied,
            reason: None,
            reason_code: None,
            enforcement_stage: None,
            normalization_version: None,
        }],
        ..PolicyDecisionEventContext::default()
    };

    let event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_007".to_string(),
        "deploy_service".to_string(),
    )
    .deny(
        reason_codes::P_RESTRICT_SCOPE,
        Some("scope target mismatch".to_string()),
    )
    .with_policy_context(context);

    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::PolicyDeny)
    );
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::EnforcementDeny)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::RuntimeEnforcement)
    );
    assert_eq!(
        event.data.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
    assert_eq!(event.data.obligation_applied_present, Some(true));
    assert_eq!(event.data.obligation_skipped_present, Some(false));
    assert_eq!(event.data.obligation_error_present, Some(false));
    assert_eq!(event.data.obligation_outcomes.len(), 1);
    assert_eq!(
        event.data.obligation_outcomes[0].reason_code.as_deref(),
        Some("obligation_applied")
    );
    assert_eq!(
        event.data.obligation_outcomes[0]
            .enforcement_stage
            .as_deref(),
        Some("handler")
    );
    assert_eq!(
        event.data.obligation_outcomes[0]
            .normalization_version
            .as_deref(),
        Some("v1")
    );
}

#[test]
fn fulfillment_sets_fail_closed_deny_path() {
    let context = PolicyDecisionEventContext {
        fail_closed: Some(FailClosedContext {
            tool_risk_class: ToolRiskClass::HighRisk,
            fail_closed_mode: FailClosedMode::FailClosed,
            fail_closed_trigger: Some(FailClosedTrigger::RuntimeDependencyError),
            fail_closed_applied: true,
            fail_closed_error_code: Some("fail_closed_runtime_dependency_error".to_string()),
        }),
        ..PolicyDecisionEventContext::default()
    };

    let event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_008".to_string(),
        "deploy_service".to_string(),
    )
    .deny(
        reason_codes::S_DB_ERROR,
        Some("store unavailable".to_string()),
    )
    .with_policy_context(context);

    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::FailClosedDeny)
    );
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::FailClosedDeny)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::FailClosedMatrix)
    );
}

#[test]
fn fulfillment_sets_policy_deny_convergence_fields() {
    let event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_009".to_string(),
        "deploy_service".to_string(),
    )
    .deny(
        reason_codes::P_POLICY_DENY,
        Some("policy blocked".to_string()),
    );

    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::PolicyDeny)
    );
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::PolicyDeny)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::PolicyEngine)
    );
    assert_eq!(
        event.data.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
}

#[test]
fn fulfillment_sets_mandate_deny_convergence_fields() {
    let event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_010".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::M_EXPIRED, Some("mandate expired".to_string()));

    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::PolicyDeny)
    );
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::EnforcementDeny)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::RuntimeEnforcement)
    );
    assert_eq!(
        event.data.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
}
