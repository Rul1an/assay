use super::basis::{compute_snapshot_id, make_clean_allow_basis, make_clean_deny_basis};
use crate::attacks::memory_poison::{PoisonOutcome, PoisonResult};
use crate::report::{AttackResult, AttackStatus};
use assay_core::mcp::decision::{
    classify_replay_diff, required_context_fields_v1, ContextPayloadState, ReplayDiffBucket,
};
use std::time::Instant;

/// Poison: shift a deny baseline to look like an allow baseline.
/// A real deny candidate compared against the poisoned baseline appears "Stricter"
/// instead of "Unchanged", masking that the baseline was always deny.
pub(in crate::attacks::memory_poison) fn vector1_replay_baseline_poisoning(
    delay_turns: u32,
) -> (PoisonResult, AttackResult) {
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

/// Poison: flip deny fields to legacy non-deny via plausible compat shape.
pub(in crate::attacks::memory_poison) fn vector2_deny_convergence_poisoning(
    delay_turns: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let clean_baseline = make_clean_deny_basis();

    let mut poisoned_baseline = clean_baseline.clone();
    poisoned_baseline.policy_deny = false;
    poisoned_baseline.deny_classification_source =
        assay_core::mcp::decision::DenyClassificationSource::LegacyDecision;
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

/// Poison: inject auth_context_summary to promote Partial to Complete.
///
/// Simulates `project_context_contract()` logic: 4 required fields, partial
/// if any missing, complete if all present.
pub(in crate::attacks::memory_poison) fn vector3_context_envelope_poisoning(
    delay_turns: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let required_count = required_context_fields_v1().len() as u32;

    let clean_present = required_count - 1;
    let clean_state = if clean_present == required_count {
        ContextPayloadState::CompleteEnvelope
    } else if clean_present == 0 {
        ContextPayloadState::AbsentEnvelope
    } else {
        ContextPayloadState::PartialEnvelope
    };

    let poisoned_present = required_count;
    let poisoned_state = if poisoned_present == required_count {
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

/// Poison: inject phantom tool class into state snapshot.
///
/// Tests snapshot-hash divergence when a phantom tool class is injected.
/// The `decay_runs` parameter is report metadata only — this vector does not
/// simulate window-based purge semantics. It validates that content-addressed
/// snapshot comparison detects injected entries, regardless of window position.
pub(in crate::attacks::memory_poison) fn vector4_decay_escape(
    decay_runs: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let clean_classes: Vec<String> = vec!["read_document".to_string()];
    let poisoned_classes: Vec<String> =
        vec!["read_document".to_string(), "write_external".to_string()];

    let clean_snapshot = compute_snapshot_id(&clean_classes);
    let poisoned_snapshot = compute_snapshot_id(&poisoned_classes);

    let snapshot_diverges = clean_snapshot != poisoned_snapshot;
    let poison_retained = true;
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
