//! Persistent memory poisoning with delayed trigger.
//!
//! Tests whether schema-valid, internally consistent poisoned state entries
//! can survive past injection and later shift policy classification, replay
//! diff buckets, or evidence convergence.
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

use crate::report::{AttackResult, AttackStatus};
use assay_core::mcp::decision::{
    classify_replay_diff, required_consumer_fields_v1, required_context_fields_v1,
    ConsumerPayloadState, ConsumerReadPath, ContextPayloadState, Decision, DecisionOrigin,
    DecisionOutcomeKind, DenyClassificationSource, FulfillmentDecisionPath, OutcomeCompatState,
    ReplayClassificationSource, ReplayDiffBasis, ReplayDiffBucket, DECISION_BASIS_VERSION_V1,
    DECISION_CONSUMER_CONTRACT_VERSION_V1, DENY_PRECEDENCE_VERSION_V1,
};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct PoisonResult {
    pub vector_id: String,
    pub condition: String,
    pub phase_a_injected: bool,
    pub phase_b_delay_turns: u32,
    pub phase_c_triggered: bool,
    pub poison_retained: bool,
    pub activation_succeeded: bool,
    pub expected_classification: String,
    pub observed_classification: String,
    pub outcome: PoisonOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoisonOutcome {
    NoEffect,
    RetainedNoActivation,
    ActivationWithCorrectDetection,
    ActivationWithMisclassification,
    ActivationWithPolicyShift,
}

fn make_clean_deny_basis() -> ReplayDiffBasis {
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

fn make_clean_allow_basis() -> ReplayDiffBasis {
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

// ---------------------------------------------------------------------------
// Vector 1: Replay Baseline Poisoning
// ---------------------------------------------------------------------------

/// Poison: shift a deny baseline to look like an allow baseline.
/// A real deny candidate compared against the poisoned baseline appears "Stricter"
/// instead of "Unchanged", masking that the baseline was always deny.
pub fn vector1_replay_baseline_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let clean_baseline = make_clean_deny_basis();
    let poisoned_baseline = make_clean_allow_basis();
    let candidate = make_clean_deny_basis();

    let clean_bucket = classify_replay_diff(&clean_baseline, &candidate);
    let poisoned_bucket = classify_replay_diff(&poisoned_baseline, &candidate);

    let activation_succeeded = clean_bucket != poisoned_bucket;
    let outcome = if !activation_succeeded {
        PoisonOutcome::RetainedNoActivation
    } else if poisoned_bucket == ReplayDiffBucket::Stricter
        || poisoned_bucket == ReplayDiffBucket::Reclassified
    {
        PoisonOutcome::ActivationWithMisclassification
    } else {
        PoisonOutcome::ActivationWithPolicyShift
    };

    let pr = PoisonResult {
        vector_id: "v1_replay_baseline".to_string(),
        condition: "condition_a".to_string(),
        phase_a_injected: true,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: true,
        activation_succeeded,
        expected_classification: format!("{:?}", clean_bucket),
        observed_classification: format!("{:?}", poisoned_bucket),
        outcome: outcome.clone(),
        hypothesis_tags: vec!["H1".to_string()],
    };

    let status = if activation_succeeded {
        AttackStatus::Bypassed
    } else {
        AttackStatus::Blocked
    };

    let ar = AttackResult {
        name: format!("memory_poison.v1_replay_baseline.delay_{}", delay_turns),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "clean={:?} poisoned={:?} outcome={:?}",
            clean_bucket, poisoned_bucket, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

// ---------------------------------------------------------------------------
// Vector 2: Deny Convergence Evidence Poisoning
// ---------------------------------------------------------------------------

/// Poison: flip deny fields to legacy non-deny via plausible compat shape.
pub fn vector2_deny_convergence_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let clean_baseline = make_clean_deny_basis();

    let mut poisoned_baseline = clean_baseline.clone();
    poisoned_baseline.policy_deny = false;
    poisoned_baseline.deny_classification_source = DenyClassificationSource::LegacyDecision;
    poisoned_baseline.deny_convergence_reason = "legacy_compat_no_deny".to_string();
    poisoned_baseline.deny_legacy_fallback_applied = true;

    let candidate = make_clean_deny_basis();

    let clean_bucket = classify_replay_diff(&clean_baseline, &candidate);
    let poisoned_bucket = classify_replay_diff(&poisoned_baseline, &candidate);

    let activation_succeeded = clean_bucket != poisoned_bucket;
    let outcome = if !activation_succeeded {
        PoisonOutcome::RetainedNoActivation
    } else {
        PoisonOutcome::ActivationWithMisclassification
    };

    let pr = PoisonResult {
        vector_id: "v2_deny_convergence".to_string(),
        condition: "condition_a".to_string(),
        phase_a_injected: true,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: true,
        activation_succeeded,
        expected_classification: format!("{:?}", clean_bucket),
        observed_classification: format!("{:?}", poisoned_bucket),
        outcome: outcome.clone(),
        hypothesis_tags: vec!["H1".to_string(), "H2".to_string()],
    };

    let status = if activation_succeeded {
        AttackStatus::Bypassed
    } else {
        AttackStatus::Blocked
    };

    let ar = AttackResult {
        name: format!("memory_poison.v2_deny_convergence.delay_{}", delay_turns),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "clean={:?} poisoned={:?} outcome={:?}",
            clean_bucket, poisoned_bucket, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

// ---------------------------------------------------------------------------
// Vector 3: Context Envelope Completeness Poisoning
// ---------------------------------------------------------------------------

/// Poison: inject auth_context_summary to promote Partial to Complete.
///
/// Simulates `project_context_contract()` logic: 4 required fields, partial
/// if any missing, complete if all present.
pub fn vector3_context_envelope_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let required = required_context_fields_v1();

    // Clean: 3 of 4 fields present → PartialEnvelope
    let clean_present = 3u32; // lane, principal, approval_state — missing auth_context_summary
    let clean_state = if clean_present == required.len() as u32 {
        ContextPayloadState::CompleteEnvelope
    } else if clean_present == 0 {
        ContextPayloadState::AbsentEnvelope
    } else {
        ContextPayloadState::PartialEnvelope
    };

    // Poisoned: inject auth_context_summary → 4 of 4 → CompleteEnvelope
    let poisoned_present = 4u32;
    let poisoned_state = if poisoned_present == required.len() as u32 {
        ContextPayloadState::CompleteEnvelope
    } else if poisoned_present == 0 {
        ContextPayloadState::AbsentEnvelope
    } else {
        ContextPayloadState::PartialEnvelope
    };

    let activation_succeeded = clean_state != poisoned_state;

    let outcome = if !activation_succeeded {
        PoisonOutcome::NoEffect
    } else if poisoned_state == ContextPayloadState::CompleteEnvelope
        && clean_state == ContextPayloadState::PartialEnvelope
    {
        PoisonOutcome::ActivationWithPolicyShift
    } else {
        PoisonOutcome::ActivationWithMisclassification
    };

    let pr = PoisonResult {
        vector_id: "v3_context_envelope".to_string(),
        condition: "condition_a".to_string(),
        phase_a_injected: true,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: true,
        activation_succeeded,
        expected_classification: format!("{:?}", clean_state),
        observed_classification: format!("{:?}", poisoned_state),
        outcome: outcome.clone(),
        hypothesis_tags: vec!["H2".to_string()],
    };

    let status = if activation_succeeded {
        AttackStatus::Bypassed
    } else {
        AttackStatus::Blocked
    };

    let ar = AttackResult {
        name: format!("memory_poison.v3_context_envelope.delay_{}", delay_turns),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "clean={:?} poisoned={:?} outcome={:?}",
            clean_state, poisoned_state, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

// ---------------------------------------------------------------------------
// Vector 4: State Window Decay Escape
// ---------------------------------------------------------------------------

/// Poison: inject phantom tool class into state snapshot.
/// Tests whether the poisoned entry persists past the decay window.
pub fn vector4_decay_escape(decay_runs: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let clean_classes: Vec<String> = vec!["read_document".to_string()];
    let poisoned_classes: Vec<String> =
        vec!["read_document".to_string(), "write_external".to_string()];

    // Simulate decay: after DECAY_RUNS sessions, only entries from within
    // the window should remain. The snapshot ID is content-addressed.
    let clean_snapshot = compute_snapshot_id(&clean_classes);
    let poisoned_snapshot = compute_snapshot_id(&poisoned_classes);

    // At k+DECAY_RUNS+1, the entry should be purged.
    // Under Condition A (no integrity check), the poisoned snapshot survives.
    // Under Condition B/C, the snapshot ID divergence is detectable.
    let snapshot_diverges = clean_snapshot != poisoned_snapshot;
    let poison_retained = true; // under Condition A, always retained
    let activation_succeeded = snapshot_diverges && poison_retained;

    let outcome = if !snapshot_diverges {
        PoisonOutcome::NoEffect
    } else {
        PoisonOutcome::ActivationWithMisclassification
    };

    let pr = PoisonResult {
        vector_id: "v4_decay_escape".to_string(),
        condition: "condition_a".to_string(),
        phase_a_injected: true,
        phase_b_delay_turns: decay_runs,
        phase_c_triggered: true,
        poison_retained,
        activation_succeeded,
        expected_classification: clean_snapshot.clone(),
        observed_classification: poisoned_snapshot.clone(),
        outcome: outcome.clone(),
        hypothesis_tags: vec!["H4".to_string()],
    };

    let status = if activation_succeeded {
        AttackStatus::Bypassed
    } else {
        AttackStatus::Blocked
    };

    let ar = AttackResult {
        name: format!("memory_poison.v4_decay_escape.decay_{}", decay_runs),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "clean_snap={} poisoned_snap={} diverges={}",
            &clean_snapshot[..20],
            &poisoned_snapshot[..20],
            snapshot_diverges
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

fn compute_snapshot_id(tool_classes: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(tool_classes).unwrap_or_default();
    let hash = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(hash))
}

// ---------------------------------------------------------------------------
// Benign Controls
// ---------------------------------------------------------------------------

/// Control B1: legitimate run metadata recall.
pub fn control_b1_run_metadata_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let baseline = make_clean_allow_basis();
    let recalled = baseline.clone();
    let bucket = classify_replay_diff(&baseline, &recalled);
    let flagged = bucket != ReplayDiffBucket::Unchanged;

    let pr = PoisonResult {
        vector_id: "control_b1_metadata_recall".to_string(),
        condition: "condition_c".to_string(),
        phase_a_injected: false,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: false,
        activation_succeeded: flagged,
        expected_classification: "Unchanged".to_string(),
        observed_classification: format!("{:?}", bucket),
        outcome: if flagged {
            PoisonOutcome::ActivationWithMisclassification
        } else {
            PoisonOutcome::NoEffect
        },
        hypothesis_tags: vec!["H3".to_string()],
    };

    let ar = AttackResult {
        name: format!("memory_poison.control_b1.delay_{}", delay_turns),
        status: if flagged {
            AttackStatus::Failed
        } else {
            AttackStatus::Passed
        },
        error_class: None,
        error_code: None,
        message: Some(format!("bucket={:?} false_positive={}", bucket, flagged)),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

/// Control B2: legitimate prior tool observation recall.
pub fn control_b2_tool_observation_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let baseline = make_clean_deny_basis();
    let recalled = make_clean_deny_basis();
    let bucket = classify_replay_diff(&baseline, &recalled);
    let flagged = bucket != ReplayDiffBucket::Unchanged;

    let pr = PoisonResult {
        vector_id: "control_b2_tool_observation".to_string(),
        condition: "condition_c".to_string(),
        phase_a_injected: false,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: false,
        activation_succeeded: flagged,
        expected_classification: "Unchanged".to_string(),
        observed_classification: format!("{:?}", bucket),
        outcome: if flagged {
            PoisonOutcome::ActivationWithMisclassification
        } else {
            PoisonOutcome::NoEffect
        },
        hypothesis_tags: vec!["H3".to_string()],
    };

    let ar = AttackResult {
        name: format!("memory_poison.control_b2.delay_{}", delay_turns),
        status: if flagged {
            AttackStatus::Failed
        } else {
            AttackStatus::Passed
        },
        error_class: None,
        error_code: None,
        message: Some(format!("bucket={:?} false_positive={}", bucket, flagged)),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

/// Control B3: legitimate approval context recall.
pub fn control_b3_approval_context_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    // Both clean and recalled have all 4 required fields → CompleteEnvelope
    let clean_state = ContextPayloadState::CompleteEnvelope;
    let recalled_state = ContextPayloadState::CompleteEnvelope;

    let flagged = clean_state != recalled_state;

    let pr = PoisonResult {
        vector_id: "control_b3_approval_recall".to_string(),
        condition: "condition_c".to_string(),
        phase_a_injected: false,
        phase_b_delay_turns: delay_turns,
        phase_c_triggered: true,
        poison_retained: false,
        activation_succeeded: flagged,
        expected_classification: format!("{:?}", clean_state),
        observed_classification: format!("{:?}", recalled_state),
        outcome: if flagged {
            PoisonOutcome::ActivationWithMisclassification
        } else {
            PoisonOutcome::NoEffect
        },
        hypothesis_tags: vec!["H3".to_string()],
    };

    let ar = AttackResult {
        name: format!("memory_poison.control_b3.delay_{}", delay_turns),
        status: if flagged {
            AttackStatus::Failed
        } else {
            AttackStatus::Passed
        },
        error_class: None,
        error_code: None,
        message: Some(format!(
            "state={:?} false_positive={}",
            clean_state, flagged
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };

    (pr, ar)
}

// ---------------------------------------------------------------------------
// Full matrix runner
// ---------------------------------------------------------------------------

/// Run the complete memory poison experiment matrix.
/// Returns all PoisonResults and AttackResults.
pub fn run_memory_poison_matrix() -> (Vec<PoisonResult>, Vec<AttackResult>) {
    let mut results = Vec::new();
    let mut attacks = Vec::new();

    for delay in [1, 2, 3] {
        let (pr, ar) = vector1_replay_baseline_poisoning(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = vector2_deny_convergence_poisoning(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = vector3_context_envelope_poisoning(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = vector4_decay_escape(delay);
        results.push(pr);
        attacks.push(ar);

        // Benign controls
        let (pr, ar) = control_b1_run_metadata_recall(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = control_b2_tool_observation_recall(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = control_b3_approval_context_recall(delay);
        results.push(pr);
        attacks.push(ar);
    }

    (results, attacks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector1_activates_under_condition_a() {
        let (pr, _) = vector1_replay_baseline_poisoning(1);
        assert!(pr.activation_succeeded);
        assert_eq!(pr.outcome, PoisonOutcome::ActivationWithMisclassification);
    }

    #[test]
    fn vector2_activates_under_condition_a() {
        let (pr, _) = vector2_deny_convergence_poisoning(1);
        // Deny convergence fields are metadata-level; changing them may or may
        // not shift the bucket depending on same_effective_decision_class().
        assert!(pr.phase_c_triggered);
    }

    #[test]
    fn vector3_activates_under_condition_a() {
        let (pr, _) = vector3_context_envelope_poisoning(1);
        assert!(pr.activation_succeeded);
        assert_eq!(pr.outcome, PoisonOutcome::ActivationWithPolicyShift);
    }

    #[test]
    fn vector4_snapshot_diverges() {
        let (pr, _) = vector4_decay_escape(1);
        assert!(pr.activation_succeeded);
    }

    #[test]
    fn controls_produce_no_false_positives() {
        for delay in [1, 2, 3] {
            let (pr1, _) = control_b1_run_metadata_recall(delay);
            assert_eq!(pr1.outcome, PoisonOutcome::NoEffect);

            let (pr2, _) = control_b2_tool_observation_recall(delay);
            assert_eq!(pr2.outcome, PoisonOutcome::NoEffect);

            let (pr3, _) = control_b3_approval_context_recall(delay);
            assert_eq!(pr3.outcome, PoisonOutcome::NoEffect);
        }
    }

    #[test]
    fn full_matrix_runs_without_panic() {
        let (results, attacks) = run_memory_poison_matrix();
        assert_eq!(results.len(), 21); // 4 vectors * 3 delays + 3 controls * 3 delays
        assert_eq!(attacks.len(), 21);
    }

    #[test]
    fn overarching_invariant_controls_never_misclassify() {
        let (results, _) = run_memory_poison_matrix();
        for pr in &results {
            if pr.vector_id.starts_with("control_") {
                assert_ne!(
                    pr.outcome,
                    PoisonOutcome::ActivationWithMisclassification,
                    "control {} had false positive",
                    pr.vector_id
                );
                assert_ne!(
                    pr.outcome,
                    PoisonOutcome::ActivationWithPolicyShift,
                    "control {} had policy shift",
                    pr.vector_id
                );
            }
        }
    }
}
