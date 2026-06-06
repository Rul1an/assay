use super::fixtures::{canonical_rank, legacy_only_rank, make_converged_allow_basis};
use super::matrix::make_consumer_result;
use super::{ConsumerOutcome, ConsumerResult};
use crate::report::AttackResult;
use assay_core::mcp::decision::{ConsumerPayloadState, ConsumerReadPath};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Benign Controls
// ---------------------------------------------------------------------------

pub fn control_e1_legitimate_legacy(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    // Genuine legacy payload: only decision field, no converged markers
    let mut basis = make_converged_allow_basis();
    basis.decision_outcome_kind = None;
    basis.decision_origin = None;
    basis.fulfillment_decision_path = None;
    basis.consumer_read_path = ConsumerReadPath::LegacyDecision;
    basis.consumer_payload_state = ConsumerPayloadState::LegacyBase;

    let rank = legacy_only_rank(&basis);
    let canonical = rank; // for legacy, legacy IS canonical

    make_consumer_result(
        "control_e1_legacy",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", rank),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}

pub fn control_e2_legitimate_compat(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let mut basis = make_converged_allow_basis();
    basis.consumer_payload_state = ConsumerPayloadState::CompatibilityFallback;
    basis.consumer_fallback_applied = true;
    basis.consumer_read_path = ConsumerReadPath::CompatibilityMarkers;

    // Legitimate compat: classification should be correct
    let canonical = canonical_rank(&basis);
    let consumer = canonical_rank(&basis);

    make_consumer_result(
        "control_e2_compat",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}

pub fn control_e3_legitimate_converged(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let basis = make_converged_allow_basis();
    let canonical = canonical_rank(&basis);
    let consumer = canonical_rank(&basis);

    make_consumer_result(
        "control_e3_converged",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}
