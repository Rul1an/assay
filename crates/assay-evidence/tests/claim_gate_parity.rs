//! Parity between this crate's claim gate and the runner substrate's (ADR-048, decision 5).
//!
//! ADR-048 moved the two shared enums into `assay_common::claim`, so the vocabulary can no longer
//! drift and the enum-equality half of this file is retired: both sides now consume one type. The
//! tables did not move. `CoverageDescriptor::claim_decision_for` (runner) and
//! `coding_agent_claim_decision` (evidence) answer from different inputs — a coverage descriptor
//! there, a per-dimension coverage state and a source class here — and this file keeps their
//! overlapping readings from drifting, which a shared type cannot do on its own.
//!
//! Two gaps are known and deliberately left open, recorded in ADR-048 rather than fixed here:
//!
//! * The fidelity table, `RunnerClaimGate::for_verdict`, is **not** pinned against the evidence
//!   gate by this file or any other. `claim_support_parity.rs` pins it only to its own crate's
//!   `claim_support` projection.
//! * The complete-coverage leg below is dead: no shipped descriptor constructor satisfies the
//!   runner's completeness rule, so that test prints a notice and skips.
//!
//! `assay-runner-schema` is a dev-dependency only; the production edge ADR-048 admitted runs the
//! other way, `assay-runner-schema -> assay-common`.

use assay_evidence::{
    coding_agent_claim_decision, CodingAgentClaimKind, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass,
};
use assay_runner_schema::{ClaimGateDecision, CoverageClaimKind, CoverageDescriptor};

/// One vocabulary since ADR-048; `CoverageClaimKind` is the same type under the runner's name.
fn kinds() -> Vec<CodingAgentClaimKind> {
    vec![
        CodingAgentClaimKind::PositiveExistence,
        CodingAgentClaimKind::ExhaustiveSet,
        CodingAgentClaimKind::BoundedNegative,
    ]
}

/// The runner's public signal for "this descriptor supports complete claims".
fn allows_absence(descriptor: &CoverageDescriptor) -> bool {
    descriptor
        .claim_decision(CoverageClaimKind::BoundedNegative)
        .decision
        == ClaimGateDecision::Allowed
}

/// Nothing watched here, and no descriptor there: both must block every claim kind.
#[test]
fn no_observation_matches_missing_descriptor() {
    for coverage in [
        CodingAgentCoverageState::Absent,
        CodingAgentCoverageState::Unavailable,
    ] {
        for kind in kinds() {
            let ours = coding_agent_claim_decision(
                // The strongest source class, so any disagreement is about coverage and not vantage.
                CodingAgentSourceClass::IndependentlyObserved,
                coverage,
                kind,
            );
            let theirs = CoverageDescriptor::claim_decision_for(None, kind);
            assert!(
                ours.decision == theirs.decision,
                "{coverage:?}/{kind:?}: evidence says {:?}, runner says {:?} ({})",
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

    for kind in kinds() {
        let ours = coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Partial,
            kind,
        );
        let theirs = descriptor.claim_decision(kind);
        assert!(
            ours.decision == theirs.decision,
            "partial/{kind:?}: evidence says {:?}, runner says {:?} ({})",
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
    for kind in kinds() {
        let ours = coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Observed,
            kind,
        );
        let theirs = complete.claim_decision(kind);
        assert!(
            ours.decision == theirs.decision,
            "observed/{kind:?}: evidence says {:?}, runner says {:?} ({})",
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

/// The four public paths are re-exports of one shared type (ADR-048, decision 6): a value read
/// through any of them is assignable through any other without conversion. Before the move this
/// did not compile, which is the whole point of the move.
#[test]
fn public_claim_vocabulary_paths_are_reexports_of_one_type() {
    let decision: assay_runner_schema::ClaimGateDecision =
        assay_common::claim::ClaimDecision::Degraded;
    let same_decision: assay_evidence::CodingAgentGateDecision = decision;
    assert_eq!(same_decision, assay_common::claim::ClaimDecision::Degraded);

    let kind: assay_runner_schema::CoverageClaimKind =
        assay_common::claim::ClaimKind::BoundedNegative;
    let same_kind: assay_evidence::CodingAgentClaimKind = kind;
    assert_eq!(same_kind, assay_common::claim::ClaimKind::BoundedNegative);
}

/// Type identity, not just assignability: every former public path names the one shared type.
/// This is the intentional ADR-048 break stated positively — a downstream `From` bridge or two
/// local trait impls across the former pair now overlap, because there is no pair.
#[test]
fn every_former_path_is_the_same_type_id() {
    use std::any::TypeId;
    let decision = TypeId::of::<assay_common::claim::ClaimDecision>();
    assert_eq!(
        decision,
        TypeId::of::<assay_runner_schema::ClaimGateDecision>()
    );
    assert_eq!(
        decision,
        TypeId::of::<assay_evidence::CodingAgentGateDecision>()
    );
    assert_eq!(
        decision,
        TypeId::of::<assay_evidence::coding_agent::CodingAgentGateDecision>()
    );
    let kind = TypeId::of::<assay_common::claim::ClaimKind>();
    assert_eq!(kind, TypeId::of::<assay_runner_schema::CoverageClaimKind>());
    assert_eq!(kind, TypeId::of::<assay_evidence::CodingAgentClaimKind>());
    assert_eq!(
        kind,
        TypeId::of::<assay_evidence::coding_agent::CodingAgentClaimKind>()
    );
}
