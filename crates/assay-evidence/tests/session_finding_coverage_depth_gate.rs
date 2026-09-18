//! Gate tests for declared depth on the assay.session.coverage sibling (Refs #2422).
//!
//! Depth remaps coverage only: `retained_through < steps_total` reads as Partial.
//! `retained_through == steps_total` keeps the sibling's coverage and source_class.
//! Missing depth fields keep slice-1 behaviour. Delegation stays on
//! `session_finding_claim_decision` / `coding_agent_claim_decision`.
//!
//! Mutant kill tests:
//! - M1: treat retained == total as Observed
//! - M2: let the depth fields override source_class
//! - M3: drop the delegation and decide locally
//! - M4: make missing depth fields change slice 1 behaviour

use assay_evidence::types::{Payload, PayloadSessionCoverage, PayloadSessionFinding};
use assay_evidence::{
    session_coverage_declared_depth, session_finding_claim_decision, CodingAgentClaimCeiling,
    CodingAgentClaimKind, CodingAgentCoverageGap, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass,
};

fn sample_held_finding(rule_id: &str) -> PayloadSessionFinding {
    PayloadSessionFinding::new(rule_id, "never_after", "held", vec![], "complete", None)
}

fn coverage_sibling(
    finding_id: &str,
    rule_id: &str,
    coverage: CodingAgentCoverageState,
    source_class: CodingAgentSourceClass,
) -> PayloadSessionCoverage {
    PayloadSessionCoverage::new(finding_id, rule_id, coverage, source_class)
}

/// First failing case (RED): a producer-reported retained == total must not lift a bounded negative.
///
/// Equality keeps whatever coverage and source_class already assert. Observed from
/// ProducerReported still collapses to SelfReported; the depth pair is not a completeness proof.
#[test]
fn first_failing_case_producer_reported_retained_eq_total_must_not_lift_bounded_negative() {
    let finding = sample_held_finding("rule_depth_eq");
    let sibling = coverage_sibling(
        "evt_finding_depth_eq",
        "rule_depth_eq",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::ProducerReported,
    )
    .with_declared_depth(Some(10), Some(10));

    assert_eq!(
        session_coverage_declared_depth(&sibling),
        CodingAgentCoverageState::Observed,
        "retained == total must keep the declared coverage, not mint a new one"
    );

    let decision = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_ne!(
        decision.decision,
        CodingAgentGateDecision::Allowed,
        "a producer-reported retained == total must not lift a bounded-negative claim"
    );
    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Blocked,
        "producer-reported retained == total must stay Blocked"
    );
    assert_eq!(
        decision.gap,
        Some(CodingAgentCoverageGap::SelfReportedOnly),
        "source_class stays ProducerReported (kills M2)"
    );
    assert_eq!(
        decision.rule, "self_reported_blocks_completeness_claim",
        "must delegate to the slice-1 producer Observed path (kills M1 and M3)"
    );

    let positive = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::PositiveExistence,
    );
    assert_eq!(
        positive.decision,
        CodingAgentGateDecision::Degraded,
        "producer-reported retained == total must not mint an independent positive claim"
    );
    assert_eq!(
        positive.ceiling,
        Some(CodingAgentClaimCeiling::Asserted),
        "depth must not override source_class (kills M2)"
    );
    assert_eq!(positive.rule, "self_reported_degrades_positive_claim");

    let without_depth = coverage_sibling(
        "evt_finding_depth_eq",
        "rule_depth_eq",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::ProducerReported,
    );
    let expected = session_finding_claim_decision(
        &finding,
        Some(&without_depth),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        decision, expected,
        "retained == total must equal the no-depth sibling decision"
    );
}

/// M4 / regression: missing depth fields behave exactly as slice 1.
#[test]
fn missing_depth_fields_match_slice_1_decisions() {
    let finding = sample_held_finding("rule_depth_missing");

    let no_sibling =
        session_finding_claim_decision(&finding, None, CodingAgentClaimKind::BoundedNegative);
    assert_eq!(no_sibling.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(no_sibling.gap, Some(CodingAgentCoverageGap::PartialOnly));
    assert_eq!(no_sibling.rule, "partial_coverage_blocks_absence_claim");

    let partial = coverage_sibling(
        "evt_finding_missing_partial",
        "rule_depth_missing",
        CodingAgentCoverageState::Partial,
        CodingAgentSourceClass::BoundaryObserved,
    );
    assert_eq!(partial.steps_total, None);
    assert_eq!(partial.retained_through, None);
    let partial_absence = session_finding_claim_decision(
        &finding,
        Some(&partial),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(partial_absence.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(
        partial_absence.rule,
        "partial_coverage_blocks_absence_claim"
    );

    let producer_observed = coverage_sibling(
        "evt_finding_missing_producer",
        "rule_depth_missing",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::ProducerReported,
    );
    let producer_absence = session_finding_claim_decision(
        &finding,
        Some(&producer_observed),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(producer_absence.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(
        producer_absence.rule,
        "self_reported_blocks_completeness_claim"
    );

    let boundary_observed = coverage_sibling(
        "evt_finding_missing_boundary",
        "rule_depth_missing",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );
    let boundary_absence = session_finding_claim_decision(
        &finding,
        Some(&boundary_observed),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(boundary_absence.decision, CodingAgentGateDecision::Allowed);
    assert_eq!(boundary_absence.rule, "observed_coverage_allows_claim");
}

/// retained < total blocks absence even when coverage says Observed from a non-producer source.
#[test]
fn retained_lt_total_blocks_absence_even_when_observed_from_non_producer() {
    let finding = sample_held_finding("rule_depth_lt");
    let sibling = coverage_sibling(
        "evt_finding_depth_lt",
        "rule_depth_lt",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    )
    .with_declared_depth(Some(10), Some(3));

    assert_eq!(
        session_coverage_declared_depth(&sibling),
        CodingAgentCoverageState::Partial,
        "retained < total reads as Partial"
    );

    let absence = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(absence.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(absence.gap, Some(CodingAgentCoverageGap::PartialOnly));
    assert_eq!(absence.rule, "partial_coverage_blocks_absence_claim");

    let partial_sibling = coverage_sibling(
        "evt_finding_depth_lt",
        "rule_depth_lt",
        CodingAgentCoverageState::Partial,
        CodingAgentSourceClass::BoundaryObserved,
    );
    let expected = session_finding_claim_decision(
        &finding,
        Some(&partial_sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        absence, expected,
        "must delegate to session_finding_claim_decision on a Partial sibling (kills M3)"
    );

    let positive = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::PositiveExistence,
    );
    assert_eq!(positive.decision, CodingAgentGateDecision::Allowed);
    assert_eq!(
        positive.ceiling,
        Some(CodingAgentClaimCeiling::ObservedInPath),
        "depth remaps coverage only; source_class stays BoundaryObserved (kills M2)"
    );
    assert_eq!(positive.rule, "partial_coverage_allows_positive_claim");
}

/// M1: retained == total on Partial + independent vantage must stay Partial, not mint Observed.
#[test]
fn retained_eq_total_does_not_mint_observed_from_partial() {
    let finding = sample_held_finding("rule_depth_eq_partial");
    let sibling = coverage_sibling(
        "evt_finding_eq_partial",
        "rule_depth_eq_partial",
        CodingAgentCoverageState::Partial,
        CodingAgentSourceClass::BoundaryObserved,
    )
    .with_declared_depth(Some(8), Some(8));

    assert_eq!(
        session_coverage_declared_depth(&sibling),
        CodingAgentCoverageState::Partial,
        "retained == total must not treat Partial as Observed (kills M1)"
    );

    let decision = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(decision.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(decision.rule, "partial_coverage_blocks_absence_claim");
}

/// A one-sided depth pair is missing depth: slice-1 coverage stands.
#[test]
fn one_sided_depth_fields_behave_as_missing() {
    let finding = sample_held_finding("rule_depth_onesided");
    let only_total = coverage_sibling(
        "evt_finding_onesided",
        "rule_depth_onesided",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    )
    .with_declared_depth(Some(10), None);
    let only_retained = coverage_sibling(
        "evt_finding_onesided",
        "rule_depth_onesided",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    )
    .with_declared_depth(None, Some(3));
    let complete = coverage_sibling(
        "evt_finding_onesided",
        "rule_depth_onesided",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );

    for sibling in [&only_total, &only_retained, &complete] {
        assert_eq!(
            session_coverage_declared_depth(sibling),
            CodingAgentCoverageState::Observed
        );
        let decision = session_finding_claim_decision(
            &finding,
            Some(sibling),
            CodingAgentClaimKind::BoundedNegative,
        );
        assert_eq!(decision.decision, CodingAgentGateDecision::Allowed);
        assert_eq!(decision.rule, "observed_coverage_allows_claim");
    }
}

/// Additive wire: omitted depth stays omitted; declared depth round-trips.
#[test]
fn declared_depth_is_additive_on_the_wire() {
    let bare = coverage_sibling(
        "evt_run_depth:1",
        "never_after:credentials->network",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );
    let bare_value = serde_json::to_value(&bare).expect("serializes");
    assert!(
        bare_value.get("steps_total").is_none(),
        "absent steps_total must stay off the wire"
    );
    assert!(
        bare_value.get("retained_through").is_none(),
        "absent retained_through must stay off the wire"
    );

    let with_depth = bare.clone().with_declared_depth(Some(12), Some(7));
    let value = serde_json::to_value(&with_depth).expect("serializes depth");
    assert_eq!(value["steps_total"], 12);
    assert_eq!(value["retained_through"], 7);
    let back: PayloadSessionCoverage =
        serde_json::from_value(value.clone()).expect("deserializes depth");
    assert_eq!(back, with_depth);

    let tagged = serde_json::json!({
        "type": PayloadSessionCoverage::EVENT_TYPE,
        "payload": value,
    });
    let payload_enum: Payload =
        serde_json::from_value(tagged).expect("Payload enum parses session coverage with depth");
    match payload_enum {
        Payload::SessionCoverage(p) => assert_eq!(p, with_depth),
        other => panic!("expected SessionCoverage variant, got {other:?}"),
    }
}
