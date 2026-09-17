//! Gate tests for assay.session.coverage sibling event and claim decision (Refs #2422).
//!
//! Pass/reject matrix for the four vectors (2026-09-05 comment):
//! - partial blocks absence
//! - self-reported total blocks absence
//! - digest-chain total allows absence
//! - missing total fails closed
//!
//! Mutant kill tests:
//! - M1: treat a missing sibling as Observed
//! - M2: accept Observed from the producer's own source class
//! - M3: skip the delegation and return Allow for Observed
//! - M4: drop the rule_id join check (a sibling for another rule must not apply)

use assay_evidence::types::{Payload, PayloadSessionCoverage, PayloadSessionFinding};
use assay_evidence::{
    coding_agent_claim_decision, session_finding_claim_decision, CodingAgentClaimCeiling,
    CodingAgentClaimKind, CodingAgentCoverageGap, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass, SESSION_COVERAGE_EVENT_TYPE,
};

fn sample_held_finding(rule_id: &str) -> PayloadSessionFinding {
    PayloadSessionFinding::new(rule_id, "never_after", "held", vec![], "complete", None)
}

/// First failing case (RED): a held finding with no sibling must not allow a bounded-negative claim.
///
/// Under a missing sibling, the decision fails closed to the most restrictive coverage
/// (`Partial`), blocking absence claims.
#[test]
fn first_failing_case_held_finding_with_no_sibling_must_not_allow_bounded_negative_claim() {
    let finding = sample_held_finding("rule_after_read_creds");
    let decision =
        session_finding_claim_decision(&finding, None, CodingAgentClaimKind::BoundedNegative);

    assert_ne!(
        decision.decision,
        CodingAgentGateDecision::Allowed,
        "a held finding with no sibling must not allow a bounded-negative claim"
    );
    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Blocked,
        "missing sibling must yield Blocked for bounded-negative claim"
    );
    assert_eq!(
        decision.gap,
        Some(CodingAgentCoverageGap::PartialOnly),
        "missing sibling reads as most restrictive coverage (PartialOnly)"
    );
    assert_eq!(
        decision.rule, "partial_coverage_blocks_absence_claim",
        "must delegate to coding_agent_claim_decision PartialOnly absence rule"
    );
}

/// Vector 1: partial coverage blocks absence, while allowing positive existence.
#[test]
fn vector_1_partial_coverage_blocks_absence_and_allows_positive() {
    let finding = sample_held_finding("rule_eval_1");
    let sibling = PayloadSessionCoverage::new(
        "evt_finding_001",
        "rule_eval_1",
        CodingAgentCoverageState::Partial,
        CodingAgentSourceClass::BoundaryObserved,
    );

    let absence = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(absence.decision, CodingAgentGateDecision::Blocked);
    assert_eq!(absence.gap, Some(CodingAgentCoverageGap::PartialOnly));
    assert_eq!(absence.rule, "partial_coverage_blocks_absence_claim");

    let positive = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::PositiveExistence,
    );
    assert_eq!(positive.decision, CodingAgentGateDecision::Allowed);
    assert_eq!(
        positive.ceiling,
        Some(CodingAgentClaimCeiling::ObservedInPath)
    );
    assert_eq!(positive.gap, None);
    assert_eq!(positive.rule, "partial_coverage_allows_positive_claim");
}

/// Vector 2 / Mutant M2 kill test: self-reported total blocks absence.
///
/// M2 accepts Observed from the producer's own source class. The gate rule specifies:
/// "Observed counts only with a non-producer source class; otherwise it reads as self-reported."
#[test]
fn vector_2_self_reported_total_blocks_absence_claim() {
    let finding = sample_held_finding("rule_eval_2");
    // Producer reports Observed, but source class is ProducerReported (the producer's own class).
    let sibling = PayloadSessionCoverage::new(
        "evt_finding_002",
        "rule_eval_2",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::ProducerReported,
    );

    let decision = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Blocked,
        "Observed from ProducerReported must read as SelfReported and block absence (kills M2)"
    );
    assert_eq!(
        decision.gap,
        Some(CodingAgentCoverageGap::SelfReportedOnly),
        "gap must be SelfReportedOnly"
    );
    assert_eq!(decision.rule, "self_reported_blocks_completeness_claim");
}

/// Vector 3 / Mutant M3 kill test: digest-chain total allows absence.
///
/// Sibling has an independent position (e.g. BoundaryObserved or IndependentlyObserved).
/// M3 skips the delegation and returns Allow for Observed.
#[test]
fn vector_3_digest_chain_total_allows_absence_and_delegates() {
    let finding = sample_held_finding("rule_eval_3");
    let sibling = PayloadSessionCoverage::new(
        "evt_finding_003",
        "rule_eval_3",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );

    let decision = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Allowed,
        "boundary-observed Observed coverage allows bounded-negative claim"
    );
    assert_eq!(
        decision.ceiling,
        Some(CodingAgentClaimCeiling::ObservedInPath),
        "ceiling must be ObservedInPath"
    );
    assert_eq!(decision.gap, None);
    assert_eq!(decision.rule, "observed_coverage_allows_claim");

    // Must match exact output of coding_agent_claim_decision (kills M3)
    let expected = coding_agent_claim_decision(
        CodingAgentSourceClass::BoundaryObserved,
        CodingAgentCoverageState::Observed,
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        decision, expected,
        "must delegate to coding_agent_claim_decision without skipping"
    );

    // Also verify IndependentlyObserved source class
    let indep_sibling = PayloadSessionCoverage::new(
        "evt_finding_003b",
        "rule_eval_3",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::IndependentlyObserved,
    );
    let indep_decision = session_finding_claim_decision(
        &finding,
        Some(&indep_sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(indep_decision.decision, CodingAgentGateDecision::Allowed);
    assert_eq!(
        indep_decision.ceiling,
        Some(CodingAgentClaimCeiling::IndependentlyConfirmed),
        "ceiling must be IndependentlyConfirmed (kills M3)"
    );
    assert_eq!(indep_decision.rule, "observed_coverage_allows_claim");
}

/// Vector 4 / Mutant M1 kill test: missing total fails closed.
///
/// M1 treats a missing sibling as Observed. With M1, missing sibling would allow absence!
#[test]
fn vector_4_missing_total_fails_closed_blocks_absence() {
    let finding = sample_held_finding("rule_eval_4");
    let decision =
        session_finding_claim_decision(&finding, None, CodingAgentClaimKind::BoundedNegative);

    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Blocked,
        "missing sibling must fail closed and block absence (kills M1)"
    );
    assert_eq!(decision.gap, Some(CodingAgentCoverageGap::PartialOnly));
    assert_eq!(decision.rule, "partial_coverage_blocks_absence_claim");
}

/// Mutant M4 kill test: drop the rule_id join check (a sibling for another rule must not apply).
#[test]
fn mutant_m4_sibling_for_different_rule_id_does_not_apply() {
    let finding = sample_held_finding("rule_alpha");
    // Sibling is for "rule_beta", but provides Observed coverage at BoundaryObserved.
    let sibling = PayloadSessionCoverage::new(
        "evt_finding_m4",
        "rule_beta",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );

    let decision = session_finding_claim_decision(
        &finding,
        Some(&sibling),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(
        decision.decision,
        CodingAgentGateDecision::Blocked,
        "sibling for a different rule_id must not apply to this finding (kills M4)"
    );
    assert_eq!(
        decision.gap,
        Some(CodingAgentCoverageGap::PartialOnly),
        "unmatched rule_id fails closed to no-sibling baseline (PartialOnly)"
    );
    assert_eq!(decision.rule, "partial_coverage_blocks_absence_claim");
}

/// Serialization and deserialization round trip for PayloadSessionCoverage and Payload variant.
#[test]
fn session_coverage_payload_and_variant_round_trip() {
    assert_eq!(PayloadSessionCoverage::EVENT_TYPE, "assay.session.coverage");
    assert_eq!(SESSION_COVERAGE_EVENT_TYPE, "assay.session.coverage");

    let coverage_payload = PayloadSessionCoverage::new(
        "evt_run_1:42",
        "never_after:credentials->network",
        CodingAgentCoverageState::Observed,
        CodingAgentSourceClass::BoundaryObserved,
    );

    // Direct serde round trip
    let value = serde_json::to_value(&coverage_payload).expect("serializes to json value");
    assert_eq!(value["finding_id"], "evt_run_1:42");
    assert_eq!(value["rule_id"], "never_after:credentials->network");
    assert_eq!(value["coverage"], "observed");
    assert_eq!(value["source_class"], "boundary_observed");

    let back: PayloadSessionCoverage =
        serde_json::from_value(value.clone()).expect("deserializes from json value");
    assert_eq!(back, coverage_payload);

    // Payload enum round trip
    let tagged = serde_json::json!({
        "type": PayloadSessionCoverage::EVENT_TYPE,
        "payload": value,
    });
    let payload_enum: Payload =
        serde_json::from_value(tagged).expect("Payload enum parses session coverage");
    match payload_enum {
        Payload::SessionCoverage(p) => assert_eq!(p, coverage_payload),
        other => panic!("expected SessionCoverage variant, got {other:?}"),
    }
}
