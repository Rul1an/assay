use assay_core::mcp::decision::{
    basis_from_decision_data, classify_replay_diff, reason_codes, Decision, DecisionEvent,
    DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath, OutcomeCompatState,
    PolicyDecisionEventContext, ReplayDiffBucket,
};
use assay_core::mcp::policy::TypedPolicyDecision;

fn allow_event(policy_version: &str, policy_digest: &str) -> DecisionEvent {
    let context = PolicyDecisionEventContext {
        typed_decision: Some(TypedPolicyDecision::AllowWithObligations),
        policy_version: Some(policy_version.to_string()),
        policy_digest: Some(policy_digest.to_string()),
        ..PolicyDecisionEventContext::default()
    };

    DecisionEvent::new(
        "assay://test".to_string(),
        "tc_200".to_string(),
        "deploy_service".to_string(),
    )
    .allow(reason_codes::P_POLICY_ALLOW)
    .with_policy_context(context)
}

#[test]
fn basis_extraction_contains_frozen_fields() {
    let event = allow_event("v1", "sha1");
    let basis = basis_from_decision_data(&event.data);

    assert_eq!(
        basis.decision_outcome_kind,
        Some(DecisionOutcomeKind::ObligationSkipped)
    );
    assert_eq!(
        basis.decision_origin,
        Some(DecisionOrigin::ObligationExecutor)
    );
    assert_eq!(
        basis.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
    assert_eq!(
        basis.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::PolicyAllow)
    );
    assert_eq!(basis.reason_code, reason_codes::P_POLICY_ALLOW);
    assert_eq!(
        basis.typed_decision,
        Some(TypedPolicyDecision::AllowWithObligations)
    );
    assert_eq!(basis.policy_version.as_deref(), Some("v1"));
    assert_eq!(basis.policy_digest.as_deref(), Some("sha1"));
    assert_eq!(basis.decision, Decision::Allow);
    assert!(!basis.fail_closed_applied);
}

#[test]
fn classify_replay_diff_unchanged() {
    let event = allow_event("v1", "sha1");
    let baseline = basis_from_decision_data(&event.data);
    let candidate = basis_from_decision_data(&event.data);

    assert_eq!(
        classify_replay_diff(&baseline, &candidate),
        ReplayDiffBucket::Unchanged
    );
}

#[test]
fn classify_replay_diff_evidence_only() {
    let baseline_event = allow_event("v1", "sha1");
    let candidate_event = allow_event("v2", "sha2");

    let baseline = basis_from_decision_data(&baseline_event.data);
    let candidate = basis_from_decision_data(&candidate_event.data);

    assert_eq!(
        classify_replay_diff(&baseline, &candidate),
        ReplayDiffBucket::EvidenceOnly
    );
}

#[test]
fn classify_replay_diff_stricter_and_looser() {
    let baseline = basis_from_decision_data(&allow_event("v1", "sha1").data);
    let deny_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_201".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::P_POLICY_DENY, Some("blocked".to_string()));
    let candidate = basis_from_decision_data(&deny_event.data);

    assert_eq!(
        classify_replay_diff(&baseline, &candidate),
        ReplayDiffBucket::Stricter
    );
    assert_eq!(
        classify_replay_diff(&candidate, &baseline),
        ReplayDiffBucket::Looser
    );
}

#[test]
fn classify_replay_diff_reclassified() {
    let baseline_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_202".to_string(),
        "deploy_service".to_string(),
    )
    .deny(
        reason_codes::P_POLICY_DENY,
        Some("policy blocked".to_string()),
    );

    let mut candidate_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_203".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::S_DB_ERROR, Some("store down".to_string()));

    // Mimic fail-closed reclassification while preserving deny strictness.
    candidate_event.data.fail_closed = Some(assay_core::mcp::policy::FailClosedContext {
        tool_risk_class: assay_core::mcp::policy::ToolRiskClass::HighRisk,
        fail_closed_mode: assay_core::mcp::policy::FailClosedMode::FailClosed,
        fail_closed_trigger: Some(
            assay_core::mcp::policy::FailClosedTrigger::RuntimeDependencyError,
        ),
        fail_closed_applied: true,
        fail_closed_error_code: Some("fail_closed_runtime_dependency_error".to_string()),
    });
    // Refresh normalization by reapplying deny with the same reason.
    candidate_event =
        candidate_event.deny(reason_codes::S_DB_ERROR, Some("store down".to_string()));

    let baseline = basis_from_decision_data(&baseline_event.data);
    let candidate = basis_from_decision_data(&candidate_event.data);

    assert_eq!(
        classify_replay_diff(&baseline, &candidate),
        ReplayDiffBucket::Reclassified
    );
}

#[test]
fn classify_replay_diff_legacy_decision_fallback() {
    let allow_event = allow_event("v1", "sha1");
    let deny_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_204".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::P_POLICY_DENY, Some("blocked".to_string()));

    let mut baseline = basis_from_decision_data(&allow_event.data);
    baseline.decision_outcome_kind = None;
    baseline.decision_origin = None;
    baseline.outcome_compat_state = None;
    baseline.fulfillment_decision_path = None;

    let mut candidate = basis_from_decision_data(&deny_event.data);
    candidate.decision_outcome_kind = None;
    candidate.decision_origin = None;
    candidate.outcome_compat_state = None;
    candidate.fulfillment_decision_path = None;

    assert_eq!(
        classify_replay_diff(&baseline, &candidate),
        ReplayDiffBucket::Stricter
    );
    assert_eq!(
        classify_replay_diff(&candidate, &baseline),
        ReplayDiffBucket::Looser
    );
}
