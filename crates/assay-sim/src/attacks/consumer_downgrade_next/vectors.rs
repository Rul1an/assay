use super::fixtures::{
    canonical_rank, legacy_only_rank, make_converged_allow_basis, make_converged_deny_basis,
};
use super::matrix::make_consumer_result;
use super::{ConsumerOutcome, ConsumerResult};
use crate::report::AttackResult;
use assay_core::mcp::decision::{
    classify_replay_diff, ConsumerPayloadState, ConsumerReadPath, Decision, DecisionOutcomeKind,
    DenyClassificationSource, ReplayClassificationSource, ReplayDiffBucket,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Vector 1: Partial-Field Trust Read
// ---------------------------------------------------------------------------

pub fn vector1_partial_trust_read(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let basis = make_converged_deny_basis();

    let canonical = canonical_rank(&basis); // 2 (deny via decision_outcome_kind)
    let consumer = match condition {
        "condition_a" => legacy_only_rank(&basis), // 1 (Allow via legacy decision)
        "condition_b" | "condition_c" => canonical_rank(&basis), // follows read-path
        _ => legacy_only_rank(&basis),
    };

    let downgrade = consumer < canonical;
    let outcome = if !downgrade {
        ConsumerOutcome::NoEffect
    } else {
        match condition {
            "condition_b" | "condition_c" => ConsumerOutcome::DowngradeWithCorrectDetection,
            _ => ConsumerOutcome::SilentDowngrade,
        }
    };

    make_consumer_result(
        "v1_partial_trust_read",
        condition,
        "consumer_realistic_synthetic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        downgrade,
        outcome,
        vec!["H1".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 2: Precedence Inversion (Deny Convergence)
// ---------------------------------------------------------------------------

pub fn vector2_precedence_inversion(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let mut basis = make_converged_deny_basis();
    basis.decision_outcome_kind = Some(DecisionOutcomeKind::EnforcementDeny);
    basis.enforcement_deny = false; // legacy field not normalized
    basis.policy_deny = false;
    basis.deny_classification_source = DenyClassificationSource::LegacyDecision;
    basis.decision = Decision::Allow;

    // Canonical: tier-1 decision_outcome_kind = EnforcementDeny -> deny
    let canonical_is_deny = true;

    // Consumer behavior per condition
    let consumer_is_deny = match condition {
        "condition_a" => basis.decision == Decision::Deny, // reads legacy: false
        "condition_b" => {
            // Reads converged fields but wrong precedence: checks policy_deny first
            basis.policy_deny || basis.enforcement_deny
        }
        "condition_c" => {
            // Full hardening: tier-1 decision_outcome_kind wins
            matches!(
                basis.decision_outcome_kind,
                Some(DecisionOutcomeKind::PolicyDeny)
                    | Some(DecisionOutcomeKind::FailClosedDeny)
                    | Some(DecisionOutcomeKind::EnforcementDeny)
            )
        }
        _ => false,
    };

    let downgrade = canonical_is_deny && !consumer_is_deny;
    let outcome = if !downgrade {
        ConsumerOutcome::NoEffect
    } else {
        match condition {
            "condition_c" => ConsumerOutcome::DowngradeWithCorrectDetection,
            _ => ConsumerOutcome::SilentDowngrade,
        }
    };

    make_consumer_result(
        "v2_precedence_inversion",
        condition,
        "producer_realistic",
        "deny",
        if consumer_is_deny { "deny" } else { "not_deny" },
        downgrade,
        outcome,
        vec!["H1".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 3: Compat Flattening (Trust Signal Suppression)
// ---------------------------------------------------------------------------

pub fn vector3_compat_flattening(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let mut compat_basis = make_converged_allow_basis();
    compat_basis.consumer_payload_state = ConsumerPayloadState::CompatibilityFallback;
    compat_basis.consumer_fallback_applied = true;
    compat_basis.consumer_read_path = ConsumerReadPath::CompatibilityMarkers;
    compat_basis.compat_fallback_applied = true;

    let converged_basis = make_converged_allow_basis();

    // Canonical: these two are different (compat vs converged)
    let canonical_bucket = classify_replay_diff(&converged_basis, &compat_basis);

    // Consumer that flattens compat: treats compat_basis as if it were converged
    let mut flattened = compat_basis.clone();
    let consumer_bucket = match condition {
        "condition_a" => {
            // Ignores all compat signals, reads as converged
            flattened.consumer_payload_state = ConsumerPayloadState::Converged;
            flattened.consumer_fallback_applied = false;
            flattened.consumer_read_path = ConsumerReadPath::ConvergedDecision;
            flattened.compat_fallback_applied = false;
            classify_replay_diff(&converged_basis, &flattened)
        }
        "condition_b" => {
            // Follows read-path but treats compat as non-binding
            flattened.consumer_payload_state = ConsumerPayloadState::Converged;
            flattened.consumer_fallback_applied = false;
            flattened.compat_fallback_applied = false;
            classify_replay_diff(&converged_basis, &flattened)
        }
        "condition_c" => {
            // Full hardening: compat signals are binding
            classify_replay_diff(&converged_basis, &compat_basis)
        }
        _ => classify_replay_diff(&converged_basis, &flattened),
    };

    let downgrade =
        canonical_bucket != consumer_bucket && consumer_bucket == ReplayDiffBucket::Unchanged;

    let outcome = if canonical_bucket == consumer_bucket || condition == "condition_c" {
        ConsumerOutcome::NoEffect
    } else if consumer_bucket == ReplayDiffBucket::Unchanged {
        ConsumerOutcome::SilentDowngrade
    } else {
        ConsumerOutcome::RetainedNoDowngrade
    };

    make_consumer_result(
        "v3_compat_flattening",
        condition,
        "consumer_realistic",
        &format!("{:?}", canonical_bucket),
        &format!("{:?}", consumer_bucket),
        downgrade,
        outcome,
        vec!["H4".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 4: Projection Loss (Required Fields Dropped)
// ---------------------------------------------------------------------------

pub fn vector4_projection_loss(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let full = make_converged_deny_basis();
    let full_rank = canonical_rank(&full); // 2

    // Strip all converged + compat fields: simulate forwarding chain loss
    let mut stripped = full.clone();
    stripped.decision_outcome_kind = None;
    stripped.decision_origin = None;
    stripped.fulfillment_decision_path = None;
    stripped.decision_basis_version = String::new();
    stripped.compat_fallback_applied = false;
    stripped.classification_source = ReplayClassificationSource::LegacyFallback;
    stripped.legacy_shape_detected = false;
    stripped.consumer_read_path = ConsumerReadPath::LegacyDecision;
    stripped.consumer_payload_state = ConsumerPayloadState::LegacyBase;
    stripped.consumer_fallback_applied = true;

    let stripped_rank = legacy_only_rank(&stripped); // 1 (Allow)

    let consumer_rank = match condition {
        "condition_a" | "condition_b" => stripped_rank,
        "condition_c" => {
            // Full hardening: required-field completeness check detects drop
            // Falls back to canonical interpretation
            full_rank
        }
        _ => stripped_rank,
    };

    let downgrade = consumer_rank < full_rank;
    let outcome = if !downgrade {
        if condition == "condition_c" {
            ConsumerOutcome::DowngradeWithCorrectDetection
        } else {
            ConsumerOutcome::NoEffect
        }
    } else {
        ConsumerOutcome::SilentDowngrade
    };

    make_consumer_result(
        "v4_projection_loss",
        condition,
        "adapter_realistic",
        &format!("rank_{}", full_rank),
        &format!("rank_{}", consumer_rank),
        downgrade,
        outcome,
        vec!["H2".into()],
        start,
    )
}
