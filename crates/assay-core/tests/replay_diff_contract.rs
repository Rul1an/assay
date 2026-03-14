use assay_core::mcp::decision::{
    basis_from_decision_data, classify_replay_diff, reason_codes, ConsumerPayloadState,
    ConsumerReadPath, Decision, DecisionEvent, DecisionOrigin, DecisionOutcomeKind,
    DenyClassificationSource, FulfillmentDecisionPath, OutcomeCompatState,
    PolicyDecisionEventContext, ReplayClassificationSource, ReplayDiffBucket,
    DECISION_BASIS_VERSION_V1, DECISION_CONSUMER_CONTRACT_VERSION_V1, DENY_PRECEDENCE_VERSION_V1,
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
    assert_eq!(basis.decision_basis_version, DECISION_BASIS_VERSION_V1);
    assert!(!basis.compat_fallback_applied);
    assert_eq!(
        basis.classification_source,
        ReplayClassificationSource::ConvergedOutcome
    );
    assert_eq!(basis.replay_diff_reason, "converged_obligation_skipped");
    assert!(!basis.legacy_shape_detected);
    assert_eq!(
        basis.decision_consumer_contract_version,
        DECISION_CONSUMER_CONTRACT_VERSION_V1
    );
    assert_eq!(
        basis.consumer_read_path,
        ConsumerReadPath::ConvergedDecision
    );
    assert!(!basis.consumer_fallback_applied);
    assert_eq!(
        basis.consumer_payload_state,
        ConsumerPayloadState::Converged
    );
    assert_eq!(
        basis.required_consumer_fields,
        vec![
            "decision".to_string(),
            "reason_code".to_string(),
            "decision_outcome_kind".to_string(),
            "decision_origin".to_string(),
            "fulfillment_decision_path".to_string(),
            "decision_basis_version".to_string(),
        ]
    );
    assert!(!basis.policy_deny);
    assert!(!basis.fail_closed_deny);
    assert!(!basis.enforcement_deny);
    assert_eq!(basis.deny_precedence_version, DENY_PRECEDENCE_VERSION_V1);
    assert_eq!(
        basis.deny_classification_source,
        DenyClassificationSource::OutcomeKind
    );
    assert!(!basis.deny_legacy_fallback_applied);
    assert_eq!(basis.deny_convergence_reason, "outcome_not_deny");
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
fn basis_extraction_prefers_fulfillment_path_when_converged_markers_missing() {
    let mut event = allow_event("v1", "sha1");
    event.data.decision_outcome_kind = None;
    event.data.decision_origin = None;
    event.data.outcome_compat_state = None;
    event.data.decision_basis_version = None;
    event.data.compat_fallback_applied = None;
    event.data.classification_source = None;
    event.data.replay_diff_reason = None;
    event.data.legacy_shape_detected = None;
    event.data.decision_consumer_contract_version = None;
    event.data.consumer_read_path = None;
    event.data.consumer_fallback_applied = None;
    event.data.consumer_payload_state = None;
    event.data.required_consumer_fields.clear();
    event.data.policy_deny = None;
    event.data.fail_closed_deny = None;
    event.data.enforcement_deny = None;
    event.data.deny_precedence_version = None;
    event.data.deny_classification_source = None;
    event.data.deny_legacy_fallback_applied = None;
    event.data.deny_convergence_reason = None;

    let basis = basis_from_decision_data(&event.data);
    assert_eq!(basis.decision_basis_version, DECISION_BASIS_VERSION_V1);
    assert!(basis.compat_fallback_applied);
    assert_eq!(
        basis.classification_source,
        ReplayClassificationSource::FulfillmentPath
    );
    assert_eq!(basis.replay_diff_reason, "fulfillment_policy_allow");
    assert!(basis.legacy_shape_detected);
    assert_eq!(
        basis.decision_consumer_contract_version,
        DECISION_CONSUMER_CONTRACT_VERSION_V1
    );
    assert_eq!(
        basis.consumer_read_path,
        ConsumerReadPath::CompatibilityMarkers
    );
    assert!(basis.consumer_fallback_applied);
    assert_eq!(
        basis.consumer_payload_state,
        ConsumerPayloadState::CompatibilityFallback
    );
    assert_eq!(
        basis.deny_classification_source,
        DenyClassificationSource::FulfillmentPath
    );
    assert!(basis.deny_legacy_fallback_applied);
    assert_eq!(basis.deny_convergence_reason, "fulfillment_policy_allow");
}

#[test]
fn basis_extraction_marks_legacy_fallback_when_shape_is_missing() {
    let mut event = allow_event("v1", "sha1");
    event.data.decision_outcome_kind = None;
    event.data.decision_origin = None;
    event.data.outcome_compat_state = None;
    event.data.fulfillment_decision_path = None;
    event.data.decision_basis_version = None;
    event.data.compat_fallback_applied = None;
    event.data.classification_source = None;
    event.data.replay_diff_reason = None;
    event.data.legacy_shape_detected = None;
    event.data.decision_consumer_contract_version = None;
    event.data.consumer_read_path = None;
    event.data.consumer_fallback_applied = None;
    event.data.consumer_payload_state = None;
    event.data.required_consumer_fields.clear();
    event.data.policy_deny = None;
    event.data.fail_closed_deny = None;
    event.data.enforcement_deny = None;
    event.data.deny_precedence_version = None;
    event.data.deny_classification_source = None;
    event.data.deny_legacy_fallback_applied = None;
    event.data.deny_convergence_reason = None;

    let basis = basis_from_decision_data(&event.data);
    assert_eq!(basis.decision_basis_version, DECISION_BASIS_VERSION_V1);
    assert!(basis.compat_fallback_applied);
    assert_eq!(
        basis.classification_source,
        ReplayClassificationSource::LegacyFallback
    );
    assert_eq!(basis.replay_diff_reason, "legacy_decision_allow");
    assert!(basis.legacy_shape_detected);
    assert_eq!(
        basis.decision_consumer_contract_version,
        DECISION_CONSUMER_CONTRACT_VERSION_V1
    );
    assert_eq!(
        basis.consumer_read_path,
        ConsumerReadPath::CompatibilityMarkers
    );
    assert!(basis.consumer_fallback_applied);
    assert_eq!(
        basis.consumer_payload_state,
        ConsumerPayloadState::CompatibilityFallback
    );
    assert_eq!(
        basis.deny_classification_source,
        DenyClassificationSource::NotDeny
    );
    assert!(basis.deny_legacy_fallback_applied);
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

#[test]
fn basis_extracts_deny_convergence_for_policy_deny() {
    let deny_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_205".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::P_POLICY_DENY, Some("blocked".to_string()));

    let basis = basis_from_decision_data(&deny_event.data);
    assert!(basis.policy_deny);
    assert!(!basis.fail_closed_deny);
    assert!(!basis.enforcement_deny);
    assert_eq!(
        basis.deny_classification_source,
        DenyClassificationSource::OutcomeKind
    );
    assert!(!basis.deny_legacy_fallback_applied);
    assert_eq!(basis.deny_convergence_reason, "outcome_policy_deny");
}

#[test]
fn basis_extracts_deny_convergence_legacy_fallback_for_missing_shape() {
    let mut deny_event = DecisionEvent::new(
        "assay://test".to_string(),
        "tc_206".to_string(),
        "deploy_service".to_string(),
    )
    .deny(reason_codes::P_POLICY_DENY, Some("blocked".to_string()));
    deny_event.data.decision_outcome_kind = None;
    deny_event.data.decision_origin = None;
    deny_event.data.outcome_compat_state = None;
    deny_event.data.fulfillment_decision_path = None;
    deny_event.data.policy_deny = None;
    deny_event.data.fail_closed_deny = None;
    deny_event.data.enforcement_deny = None;
    deny_event.data.deny_precedence_version = None;
    deny_event.data.deny_classification_source = None;
    deny_event.data.deny_legacy_fallback_applied = None;
    deny_event.data.deny_convergence_reason = None;

    let basis = basis_from_decision_data(&deny_event.data);
    assert!(basis.policy_deny);
    assert!(!basis.fail_closed_deny);
    assert!(!basis.enforcement_deny);
    assert_eq!(basis.deny_precedence_version, DENY_PRECEDENCE_VERSION_V1);
    assert_eq!(
        basis.deny_classification_source,
        DenyClassificationSource::LegacyDecision
    );
    assert!(basis.deny_legacy_fallback_applied);
    assert_eq!(basis.deny_convergence_reason, "legacy_policy_deny");
}
