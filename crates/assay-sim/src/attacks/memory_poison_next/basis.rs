use assay_core::mcp::decision::{
    required_consumer_fields_v1, ConsumerPayloadState, ConsumerReadPath, Decision, DecisionOrigin,
    DecisionOutcomeKind, DenyClassificationSource, FulfillmentDecisionPath, OutcomeCompatState,
    ReplayClassificationSource, ReplayDiffBasis, DECISION_BASIS_VERSION_V1,
    DECISION_CONSUMER_CONTRACT_VERSION_V1, DENY_PRECEDENCE_VERSION_V1,
};

pub(in crate::attacks::memory_poison) fn make_clean_deny_basis() -> ReplayDiffBasis {
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
        deny_convergence_reason: "policy_rule_match".to_string(),
        reason_code: "policy_deny_sensitive_tool".to_string(),
        typed_decision: None,
        policy_version: Some("v1".to_string()),
        policy_digest: Some("sha256:abc".to_string()),
        decision: Decision::Deny,
        fail_closed_applied: false,
    }
}

pub(in crate::attacks::memory_poison) fn make_clean_allow_basis() -> ReplayDiffBasis {
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

pub(in crate::attacks::memory_poison) fn compute_snapshot_id(tool_classes: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(tool_classes)
        .expect("snapshot serialization is infallible for Vec<String>");
    let hash = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(hash))
}

pub(in crate::attacks::memory_poison) fn condition_b_replay_integrity(
    clean: &ReplayDiffBasis,
    candidate: &ReplayDiffBasis,
) -> bool {
    match (compute_basis_hash(clean), compute_basis_hash(candidate)) {
        (Ok(clean_hash), Ok(candidate_hash)) => clean_hash == candidate_hash,
        _ => false,
    }
}

pub(in crate::attacks::memory_poison) fn compute_basis_hash(
    basis: &ReplayDiffBasis,
) -> Result<String, serde_json::Error> {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(basis)?;
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hash)))
}
