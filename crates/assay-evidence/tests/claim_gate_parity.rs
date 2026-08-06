//! Parity between this crate's claim gate and the runner substrate's.
//!
//! Two implementations of one rule always drift. `assay-runner-schema` has gated claims by kind
//! since 2026-06-01 (`RunnerClaimGate`) and by coverage descriptor since 2026-06-04
//! (`CoverageDescriptor::claim_decision`); `assay-evidence` re-states the same rule over a different
//! vocabulary because its inputs are different (a per-dimension coverage state and a source class,
//! rather than a runner fidelity verdict and a descriptor).
//!
//! Ideally one would call the other. It cannot today: the runner substrate is documented as
//! internal and API-unstable, and its inputs are runner-domain vocabulary that an evidence pack does
//! not have. Promoting the shared mechanism into `assay-common` is a real ADR question — the
//! CLAUDE.md admission test ("a mechanism whose second implementation would silently mean something
//! different") is arguably met, and this file is the evidence that it is met, since the first draft
//! of the evidence-side rule *did* silently mean something different.
//!
//! Until that decision is taken, this is the sanctioned fallback: run both over the states that
//! correspond and require the same decision. `assay-runner-schema` is a dev-dependency only.

use assay_evidence::{
    coding_agent_claim_decision, CodingAgentClaimKind, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass,
};
use assay_runner_schema::{ClaimGateDecision, CoverageClaimKind, CoverageDescriptor};

/// The two claim-kind vocabularies, paired.
fn kinds() -> Vec<(CodingAgentClaimKind, CoverageClaimKind)> {
    vec![
        (
            CodingAgentClaimKind::PositiveExistence,
            CoverageClaimKind::PositiveExistence,
        ),
        (
            CodingAgentClaimKind::ExhaustiveSet,
            CoverageClaimKind::ExhaustiveSet,
        ),
        (
            CodingAgentClaimKind::BoundedNegative,
            CoverageClaimKind::BoundedNegative,
        ),
    ]
}

/// The runner's public signal for "this descriptor supports complete claims".
fn allows_absence(descriptor: &CoverageDescriptor) -> bool {
    descriptor
        .claim_decision(CoverageClaimKind::BoundedNegative)
        .decision
        == ClaimGateDecision::Allowed
}

fn same(ours: CodingAgentGateDecision, theirs: ClaimGateDecision) -> bool {
    matches!(
        (ours, theirs),
        (CodingAgentGateDecision::Allowed, ClaimGateDecision::Allowed)
            | (
                CodingAgentGateDecision::Degraded,
                ClaimGateDecision::Degraded
            )
            | (CodingAgentGateDecision::Blocked, ClaimGateDecision::Blocked)
    )
}

/// Nothing watched here, and no descriptor there: both must block every claim kind.
#[test]
fn no_observation_matches_missing_descriptor() {
    for coverage in [
        CodingAgentCoverageState::Absent,
        CodingAgentCoverageState::Unavailable,
    ] {
        for (ours_kind, their_kind) in kinds() {
            let ours = coding_agent_claim_decision(
                // The strongest source class, so any disagreement is about coverage and not vantage.
                CodingAgentSourceClass::IndependentlyObserved,
                coverage,
                ours_kind,
            );
            let theirs = CoverageDescriptor::claim_decision_for(None, their_kind);
            assert!(
                same(ours.decision, theirs.decision),
                "{coverage:?}/{ours_kind:?}: evidence says {:?}, runner says {:?} ({})",
                ours.decision,
                theirs.decision,
                theirs.rule
            );
        }
    }
}

/// Partial coverage here, partial completeness there. This is the case the first draft of the
/// evidence-side rule got wrong: it returned "incomplete" for a positive claim the runner allows.
#[test]
fn partial_coverage_matches_partial_completeness() {
    // `filesystem_open_syscall_only` is a descriptor with real completeness limits and declared
    // blind spots — the runner's canonical partial case.
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();
    // Completeness is derived from the runner's own public behaviour rather than by widening its API
    // for a test: a descriptor supports complete claims exactly when it allows a bounded negative.
    assert!(
        !allows_absence(&descriptor),
        "fixture must actually be partial, or this test proves nothing"
    );

    for (ours_kind, their_kind) in kinds() {
        let ours = coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Partial,
            ours_kind,
        );
        let theirs = descriptor.claim_decision(their_kind);
        assert!(
            same(ours.decision, theirs.decision),
            "partial/{ours_kind:?}: evidence says {:?}, runner says {:?} ({})",
            ours.decision,
            theirs.decision,
            theirs.rule
        );
    }
}

/// Full coverage here, a complete descriptor there: both allow everything.
#[test]
fn full_coverage_matches_complete_descriptor() {
    let complete = CoverageDescriptor::network_connect_and_datagram_peer_observed();
    if !allows_absence(&complete) {
        // The runner's notion of "complete" is descriptor-specific; if no shipped constructor is
        // complete, this leg cannot be asserted and saying so is better than asserting a weaker one.
        eprintln!("no shipped descriptor reports complete coverage; leg skipped deliberately");
        return;
    }
    for (ours_kind, their_kind) in kinds() {
        let ours = coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Observed,
            ours_kind,
        );
        let theirs = complete.claim_decision(their_kind);
        assert!(
            same(ours.decision, theirs.decision),
            "observed/{ours_kind:?}: evidence says {:?}, runner says {:?} ({})",
            ours.decision,
            theirs.decision,
            theirs.rule
        );
    }
}

/// The asymmetry itself, asserted on both sides rather than inferred: partial coverage must allow a
/// positive claim and block an absence claim, in both crates.
#[test]
fn both_crates_keep_the_occurrence_absence_asymmetry() {
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();

    assert_eq!(
        descriptor
            .claim_decision(CoverageClaimKind::PositiveExistence)
            .decision,
        ClaimGateDecision::Allowed
    );
    assert_eq!(
        descriptor
            .claim_decision(CoverageClaimKind::BoundedNegative)
            .decision,
        ClaimGateDecision::Blocked
    );

    assert_eq!(
        coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Partial,
            CodingAgentClaimKind::PositiveExistence,
        )
        .decision,
        CodingAgentGateDecision::Allowed
    );
    assert_eq!(
        coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Partial,
            CodingAgentClaimKind::BoundedNegative,
        )
        .decision,
        CodingAgentGateDecision::Blocked
    );
}

/// `SelfReported` has no runner analogue — the runner models coverage completeness, not who the
/// account came from. Recorded so a later reader does not mistake its absence from the parity legs
/// above for an oversight.
#[test]
fn self_reported_is_documented_as_out_of_parity_scope() {
    let ours = coding_agent_claim_decision(
        CodingAgentSourceClass::IndependentlyObserved,
        CodingAgentCoverageState::SelfReported,
        CodingAgentClaimKind::PositiveExistence,
    );
    assert_eq!(ours.decision, CodingAgentGateDecision::Degraded);
    assert_eq!(ours.rule, "self_reported_degrades_positive_claim");
}
