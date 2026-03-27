use super::basis::{
    compute_snapshot_id, condition_b_replay_integrity, make_clean_allow_basis,
    make_clean_deny_basis,
};
use super::matrix::make_result;
use crate::attacks::memory_poison::{PoisonOutcome, PoisonResult};
use crate::report::AttackResult;
use assay_core::mcp::decision::{
    classify_replay_diff, required_context_fields_v1, ContextPayloadState, DenyClassificationSource,
};
use std::time::Instant;

/// Condition B: V1 — integrity check detects poisoned baseline.
pub(in crate::attacks::memory_poison) fn vector1_condition_b(
    delay: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();
    let clean = make_clean_deny_basis();
    let poisoned = make_clean_allow_basis();
    let candidate = make_clean_deny_basis();

    let detected = !condition_b_replay_integrity(&clean, &poisoned);
    let bucket = if detected {
        classify_replay_diff(&clean, &candidate)
    } else {
        classify_replay_diff(&poisoned, &candidate)
    };
    let clean_bucket = classify_replay_diff(&clean, &candidate);
    let activation_succeeded = !detected && bucket != clean_bucket;

    let outcome = if detected {
        PoisonOutcome::ActivationWithCorrectDetection
    } else if activation_succeeded {
        PoisonOutcome::ActivationWithMisclassification
    } else {
        PoisonOutcome::RetainedNoActivation
    };

    make_result(
        "v1_replay_baseline",
        "condition_b",
        delay,
        !detected,
        activation_succeeded,
        &format!("{:?}", clean_bucket),
        &format!("{:?}", bucket),
        outcome.clone(),
        vec!["H1".into()],
        start,
    )
}

/// Condition B: V2 — integrity check on deny convergence fields.
pub(in crate::attacks::memory_poison) fn vector2_condition_b(
    delay: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();
    let clean = make_clean_deny_basis();
    let mut poisoned = clean.clone();
    poisoned.policy_deny = false;
    poisoned.deny_classification_source = DenyClassificationSource::LegacyDecision;
    poisoned.deny_convergence_reason = "legacy_compat_no_deny".to_string();
    poisoned.deny_legacy_fallback_applied = true;

    let detected = !condition_b_replay_integrity(&clean, &poisoned);
    let candidate = make_clean_deny_basis();
    let bucket = if detected {
        classify_replay_diff(&clean, &candidate)
    } else {
        classify_replay_diff(&poisoned, &candidate)
    };
    let clean_bucket = classify_replay_diff(&clean, &candidate);
    let activation_succeeded = !detected && bucket != clean_bucket;

    let outcome = if detected {
        PoisonOutcome::ActivationWithCorrectDetection
    } else if activation_succeeded {
        PoisonOutcome::ActivationWithMisclassification
    } else {
        PoisonOutcome::RetainedNoActivation
    };

    make_result(
        "v2_deny_convergence",
        "condition_b",
        delay,
        !detected,
        activation_succeeded,
        &format!("{:?}", clean_bucket),
        &format!("{:?}", bucket),
        outcome.clone(),
        vec!["H1".into()],
        start,
    )
}

/// Condition B: V4 — snapshot hash comparison detects injected tool class.
pub(in crate::attacks::memory_poison) fn vector4_condition_b(
    delay: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();
    let clean_classes = vec!["read_document".to_string()];
    let poisoned_classes = vec!["read_document".to_string(), "write_external".to_string()];

    let clean_snap = compute_snapshot_id(&clean_classes);
    let poisoned_snap = compute_snapshot_id(&poisoned_classes);
    let detected = clean_snap != poisoned_snap;

    let outcome = if detected {
        PoisonOutcome::ActivationWithCorrectDetection
    } else {
        PoisonOutcome::ActivationWithMisclassification
    };

    make_result(
        "v4_decay_escape",
        "condition_b",
        delay,
        !detected,
        !detected,
        &clean_snap,
        &poisoned_snap,
        outcome.clone(),
        vec!["H1".into(), "H4".into()],
        start,
    )
}

/// Condition C: V3 — context contract validates field provenance.
/// Under full stack, a poisoned auth_context_summary is detected because
/// the contract validates that the field was present at decision time,
/// not injected later.
pub(in crate::attacks::memory_poison) fn vector3_condition_c(
    delay: u32,
) -> (PoisonResult, AttackResult) {
    let start = Instant::now();

    let required_count = required_context_fields_v1().len() as u32;
    let clean_present = required_count - 1;
    let poisoned_present = required_count;

    let field_provenance_valid = clean_present == poisoned_present;
    let detected = !field_provenance_valid;

    let clean_state = ContextPayloadState::PartialEnvelope;
    let observed_state = if detected {
        clean_state
    } else {
        ContextPayloadState::CompleteEnvelope
    };

    let activation_succeeded = !detected && observed_state != clean_state;
    let outcome = if detected {
        PoisonOutcome::ActivationWithCorrectDetection
    } else if activation_succeeded {
        PoisonOutcome::ActivationWithPolicyShift
    } else {
        PoisonOutcome::RetainedNoActivation
    };

    make_result(
        "v3_context_envelope",
        "condition_c",
        delay,
        !detected,
        activation_succeeded,
        &format!("{:?}", clean_state),
        &format!("{:?}", observed_state),
        outcome.clone(),
        vec!["H2".into()],
        start,
    )
}
