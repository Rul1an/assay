use assay_evidence::{
    coding_agent_claim_ceiling, coding_agent_claim_decision, coding_agent_evidence_event,
    types::Payload, CodingAgentClaimCeiling, CodingAgentClaimKind, CodingAgentCoverage,
    CodingAgentCoverageGap, CodingAgentCoverageState, CodingAgentDeclaredScope,
    CodingAgentEvidencePayload, CodingAgentGateDecision, CodingAgentNetworkPolicy,
    CodingAgentObservedEffects, CodingAgentSourceClass, CODING_AGENT_EVIDENCE_EVENT_TYPE,
};
use serde_json::json;

fn complete_payload() -> CodingAgentEvidencePayload {
    CodingAgentEvidencePayload::new(
        CodingAgentDeclaredScope {
            allowed_files: vec!["src/foo.py".to_string()],
            allowed_commands: vec!["pytest".to_string()],
            network: CodingAgentNetworkPolicy::Denied,
            allowed_mcp_tools: vec!["fs.read".to_string()],
            expected_test_command: Some("pytest".to_string()),
            authorized: true,
        },
        CodingAgentObservedEffects {
            files_changed: vec!["src/foo.py".to_string()],
            commands_executed: vec!["pytest".to_string()],
            network_attempts: vec![],
            mcp_tool_calls: vec!["fs.read".to_string()],
            test_observed: true,
        },
        CodingAgentCoverage {
            files: CodingAgentCoverageState::Observed,
            commands: CodingAgentCoverageState::Observed,
            network: CodingAgentCoverageState::Observed,
            mcp_tools: CodingAgentCoverageState::Observed,
            test: CodingAgentCoverageState::Observed,
        },
        CodingAgentSourceClass::BoundaryObserved,
    )
}

#[test]
fn coding_agent_payload_serializes_without_verdict_fields() {
    let payload = complete_payload();
    let value = serde_json::to_value(&payload).expect("payload should serialize");

    assert!(value.get("schema").is_none());
    assert_eq!(value["source_class"], "boundary_observed");
    assert_eq!(value["declared_scope"]["network"], "denied");
    assert_eq!(value["coverage"]["network"], "observed");
    assert_eq!(
        value["non_claims"],
        json!([
            "does_not_prove_code_correctness",
            "does_not_prove_agent_intent",
            "does_not_replace_human_review"
        ])
    );
    assert!(value.get("verdict").is_none());
    assert!(value.get("effect_sufficiency").is_none());
}

#[test]
fn coding_agent_event_is_content_addressed_and_has_no_verdict() {
    let event = coding_agent_evidence_event("run_ca", 0, complete_payload())
        .expect("event should be content-addressed");

    assert_eq!(event.type_, CODING_AGENT_EVIDENCE_EVENT_TYPE);
    assert_eq!(event.run_id, "run_ca");
    assert_eq!(event.seq, 0);
    assert!(event
        .content_hash
        .as_deref()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        event.content_hash.as_deref(),
        Some("sha256:47d0f0e367e32403af9d2f53fc7b547fb8c6ba27a4cdd7c532b2d8632af4fe42")
    );

    let payload = &event.payload;
    assert!(payload.get("schema").is_none());
    assert!(payload.get("verdict").is_none());
    assert!(payload.get("effect_sufficiency").is_none());
}

#[test]
fn source_class_is_carried_as_input_not_collapsed_to_verdict() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::ProducerReported;

    let event = coding_agent_evidence_event("run_ca", 1, payload)
        .expect("producer-reported payload should still be recordable evidence");

    assert_eq!(event.payload["source_class"], "producer_reported");
    assert!(event.payload.get("verdict").is_none());
}

#[test]
fn observed_absence_is_explicit_coverage_not_missing_field() {
    let mut payload = complete_payload();
    payload.coverage.network = CodingAgentCoverageState::Absent;
    payload.observed_effects.network_attempts = vec![];

    let event = coding_agent_evidence_event("run_ca", 2, payload)
        .expect("absence coverage should serialize as explicit input");

    assert_eq!(event.payload["coverage"]["network"], "absent");
    assert_eq!(
        event.payload["observed_effects"]["network_attempts"],
        json!([])
    );
}

#[test]
fn expected_test_command_none_is_omitted() {
    let mut payload = complete_payload();
    payload.declared_scope.expected_test_command = None;

    let event = coding_agent_evidence_event("run_ca", 3, payload)
        .expect("payload without expected test command should serialize");

    assert!(event.payload["declared_scope"]
        .get("expected_test_command")
        .is_none());
}

#[test]
fn coding_agent_payload_is_available_through_typed_payload_enum() {
    let tagged = json!({
        "type": CODING_AGENT_EVIDENCE_EVENT_TYPE,
        "payload": complete_payload()
    });

    let payload: Payload =
        serde_json::from_value(tagged).expect("typed payload should deserialize");

    match payload {
        Payload::CodingAgentEvidencePack(inner) => {
            assert_eq!(inner.source_class, CodingAgentSourceClass::BoundaryObserved);
        }
        other => panic!("expected CodingAgentEvidencePack payload, got {other:?}"),
    }
}

// --- the claim gate: coverage, source class, and claim kind ---------------------------------
//
// A source class ranks where an observer sat. Coverage says whether it looked. The claim kind
// decides how much looking is enough. These pin all three, and pin that partial coverage still
// supports saying what WAS seen — the asymmetry the first draft of this module got wrong.

use CodingAgentClaimKind::{BoundedNegative, ExhaustiveSet, PositiveExistence};
use CodingAgentGateDecision::{Allowed, Blocked, Degraded};

fn coverage_all(state: CodingAgentCoverageState) -> CodingAgentCoverage {
    CodingAgentCoverage {
        files: state,
        commands: state,
        network: state,
        mcp_tools: state,
        test: state,
    }
}

#[test]
fn ceilings_match_the_published_rge_bench_ladder() {
    use CodingAgentClaimCeiling::*;
    for (source, want) in [
        (CodingAgentSourceClass::ProducerReported, Asserted),
        (CodingAgentSourceClass::IssuerAttested, AssertedSigned),
        (CodingAgentSourceClass::ReceiverReceipt, ObservedAtReceiver),
        (CodingAgentSourceClass::BoundaryObserved, ObservedInPath),
        (
            CodingAgentSourceClass::ThirdPartyObserved,
            IndependentlyConfirmed,
        ),
        (
            CodingAgentSourceClass::IndependentlyObserved,
            IndependentlyConfirmed,
        ),
    ] {
        assert_eq!(coding_agent_claim_ceiling(source), want, "{source:?}");
    }
    assert!(Asserted < AssertedSigned);
    assert!(AssertedSigned < ObservedAtReceiver);
    assert!(ObservedAtReceiver < ObservedInPath);
    assert!(ObservedInPath < IndependentlyConfirmed);
}

#[test]
fn a_receiver_receipt_is_not_promoted_to_independent() {
    assert!(
        coding_agent_claim_ceiling(CodingAgentSourceClass::ReceiverReceipt)
            < coding_agent_claim_ceiling(CodingAgentSourceClass::BoundaryObserved)
    );
}

#[test]
fn attestation_does_not_raise_the_ceiling() {
    assert!(
        coding_agent_claim_ceiling(CodingAgentSourceClass::IssuerAttested)
            < coding_agent_claim_ceiling(CodingAgentSourceClass::ReceiverReceipt)
    );
}

// --- the asymmetry the first draft got wrong -------------------------------------------------

#[test]
fn partial_coverage_allows_positive_but_blocks_absence() {
    let src = CodingAgentSourceClass::BoundaryObserved;
    let cov = CodingAgentCoverageState::Partial;

    let positive = coding_agent_claim_decision(src, cov, PositiveExistence);
    assert_eq!(
        positive.decision, Allowed,
        "seeing part of a run still says what was seen"
    );
    assert_eq!(
        positive.ceiling,
        Some(CodingAgentClaimCeiling::ObservedInPath)
    );
    assert_eq!(positive.gap, None);

    assert_eq!(
        coding_agent_claim_decision(src, cov, ExhaustiveSet).decision,
        Degraded
    );

    let absence = coding_agent_claim_decision(src, cov, BoundedNegative);
    assert_eq!(
        absence.decision, Blocked,
        "a blind spot can hide the requested absence"
    );
    assert_eq!(absence.ceiling, None, "a blocked claim has no ceiling");
    assert_eq!(absence.gap, Some(CodingAgentCoverageGap::PartialOnly));
}

#[test]
fn nothing_watched_blocks_every_claim_kind() {
    for (cov, gap) in [
        (
            CodingAgentCoverageState::Absent,
            CodingAgentCoverageGap::NotObserved,
        ),
        (
            CodingAgentCoverageState::Unavailable,
            CodingAgentCoverageGap::ObserverUnavailable,
        ),
    ] {
        for kind in [PositiveExistence, ExhaustiveSet, BoundedNegative] {
            let got = coding_agent_claim_decision(
                CodingAgentSourceClass::IndependentlyObserved,
                cov,
                kind,
            );
            assert_eq!(
                got.decision, Blocked,
                "{cov:?}/{kind:?} under the strongest source class"
            );
            assert_eq!(got.gap, Some(gap));
        }
    }
}

#[test]
fn self_reported_degrades_positive_and_blocks_completeness() {
    let src = CodingAgentSourceClass::IndependentlyObserved;
    let cov = CodingAgentCoverageState::SelfReported;

    let positive = coding_agent_claim_decision(src, cov, PositiveExistence);
    assert_eq!(positive.decision, Degraded);
    assert_eq!(
        positive.ceiling,
        Some(CodingAgentClaimCeiling::Asserted),
        "a self-reported account caps at asserted however the run is otherwise classed"
    );

    for kind in [ExhaustiveSet, BoundedNegative] {
        assert_eq!(
            coding_agent_claim_decision(src, cov, kind).decision,
            Blocked
        );
    }
}

#[test]
fn full_coverage_allows_every_kind_at_the_source_class_ceiling() {
    for source in [
        CodingAgentSourceClass::ProducerReported,
        CodingAgentSourceClass::BoundaryObserved,
        CodingAgentSourceClass::ThirdPartyObserved,
    ] {
        for kind in [PositiveExistence, ExhaustiveSet, BoundedNegative] {
            let got = coding_agent_claim_decision(source, CodingAgentCoverageState::Observed, kind);
            assert_eq!(got.decision, Allowed, "{source:?}/{kind:?}");
            assert_eq!(got.ceiling, Some(coding_agent_claim_ceiling(source)));
        }
    }
}

#[test]
fn coverage_is_evaluated_before_the_source_class() {
    // Producer-reported would be Allowed-at-asserted if it had looked. It did not, and the coverage
    // reason must survive rather than being overwritten by the weaker source class.
    let got = coding_agent_claim_decision(
        CodingAgentSourceClass::ProducerReported,
        CodingAgentCoverageState::Absent,
        PositiveExistence,
    );
    assert_eq!(got.decision, Blocked);
    assert_eq!(got.gap, Some(CodingAgentCoverageGap::NotObserved));
}

// --- the report ------------------------------------------------------------------------------

#[test]
fn undeclared_test_dimension_is_out_of_scope_not_a_gap() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::IndependentlyObserved;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    payload.coverage.test = CodingAgentCoverageState::Absent;

    payload.declared_scope.expected_test_command = None;
    let report = payload.coverage_report(BoundedNegative);
    assert!(
        report.test.is_none(),
        "a dimension nobody claimed is not a gap"
    );
    assert!(report.gaps().is_empty());
    assert!(report.meets(CodingAgentClaimCeiling::IndependentlyConfirmed));

    payload.declared_scope.expected_test_command = Some("cargo test".to_string());
    let report = payload.coverage_report(BoundedNegative);
    assert!(
        report.test.is_some(),
        "a declared test command puts the dimension in scope"
    );
    assert_eq!(report.gaps().len(), 1);
    assert_eq!(report.gaps()[0].0, "test");
    assert_eq!(report.weakest_ceiling(), None);
}

#[test]
fn each_dimension_can_sink_the_report_on_its_own() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::BoundaryObserved;
    payload.declared_scope.expected_test_command = None;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    assert!(payload
        .coverage_report(BoundedNegative)
        .meets(CodingAgentClaimCeiling::ObservedInPath));

    for name in ["files", "commands", "network", "mcp_tools"] {
        let mut p = payload.clone();
        match name {
            "files" => p.coverage.files = CodingAgentCoverageState::Absent,
            "commands" => p.coverage.commands = CodingAgentCoverageState::Absent,
            "network" => p.coverage.network = CodingAgentCoverageState::Absent,
            _ => p.coverage.mcp_tools = CodingAgentCoverageState::Absent,
        }
        let report = p.coverage_report(BoundedNegative);
        assert_eq!(
            report.weakest_ceiling(),
            None,
            "{name} unobserved did not sink the report"
        );
        assert_eq!(report.gaps().len(), 1, "{name}");
        assert_eq!(report.gaps()[0].0, name);
    }
}

#[test]
fn the_report_is_claim_kind_aware() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::BoundaryObserved;
    payload.declared_scope.expected_test_command = None;
    payload.coverage = coverage_all(CodingAgentCoverageState::Partial);

    // The same run supports "these things happened" and not "these are all of them".
    assert!(payload.coverage_report(PositiveExistence).gaps().is_empty());
    assert_eq!(payload.coverage_report(BoundedNegative).gaps().len(), 4);
}

#[test]
fn the_weakest_dimension_binds_the_report() {
    let mut payload = complete_payload();
    payload.declared_scope.expected_test_command = None;
    payload.source_class = CodingAgentSourceClass::IndependentlyObserved;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    payload.coverage.network = CodingAgentCoverageState::SelfReported;

    let report = payload.coverage_report(PositiveExistence);
    assert_eq!(
        report.weakest_ceiling(),
        Some(CodingAgentClaimCeiling::Asserted),
        "one self-reported dimension drags the whole report to its rung"
    );
    assert!(
        !report.meets(CodingAgentClaimCeiling::Asserted),
        "degraded is not clean"
    );
}
