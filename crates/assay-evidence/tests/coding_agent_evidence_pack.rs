use assay_evidence::{
    coding_agent_claim_ceiling, coding_agent_dimension_conclusion, coding_agent_evidence_event,
    types::Payload, CodingAgentClaimCeiling, CodingAgentCoverage, CodingAgentCoverageGap,
    CodingAgentCoverageState, CodingAgentDeclaredScope, CodingAgentDimensionConclusion,
    CodingAgentEvidencePayload, CodingAgentNetworkPolicy, CodingAgentObservedEffects,
    CodingAgentSourceClass, CODING_AGENT_EVIDENCE_EVENT_TYPE,
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

// --- coverage denominator + the published ceiling ladder -------------------------------------
//
// A source class ranks where an observer sat. It does not say whether that observer looked.
// These tests pin that the two axes stay separate, that a gap binds harder than any rung, and that
// the rungs match the published ladder rather than collapsing into a binary.

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
    // producer_reported (1) < issuer_attested (2) < receiver_receipt (3) < boundary_observed (4)
    //   < third_party_observed (5), against asserted < asserted_signed < observed_at_receiver
    //   < observed_in_path < independently_confirmed.
    use assay_evidence::CodingAgentClaimCeiling::*;
    let ladder = [
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
    ];
    for (source, want) in ladder {
        assert_eq!(coding_agent_claim_ceiling(source), want, "{source:?}");
    }
    assert!(Asserted < AssertedSigned);
    assert!(AssertedSigned < ObservedAtReceiver);
    assert!(ObservedAtReceiver < ObservedInPath);
    assert!(ObservedInPath < IndependentlyConfirmed);
}

#[test]
fn a_receiver_receipt_is_not_promoted_to_independent() {
    // The defect this test exists to prevent: lumping receiver_receipt in with the independent
    // classes, which over-grants a receipt exactly where our own ladder caps it.
    assert_eq!(
        coding_agent_dimension_conclusion(
            CodingAgentSourceClass::ReceiverReceipt,
            CodingAgentCoverageState::Observed,
        ),
        CodingAgentDimensionConclusion::Supported {
            ceiling: CodingAgentClaimCeiling::ObservedAtReceiver
        },
    );
    assert!(
        coding_agent_claim_ceiling(CodingAgentSourceClass::ReceiverReceipt)
            < coding_agent_claim_ceiling(CodingAgentSourceClass::BoundaryObserved)
    );
}

#[test]
fn attestation_does_not_raise_the_ceiling() {
    // issuer_attested is signed and still sits below any observing position.
    assert!(
        coding_agent_claim_ceiling(CodingAgentSourceClass::IssuerAttested)
            < coding_agent_claim_ceiling(CodingAgentSourceClass::ReceiverReceipt)
    );
}

#[test]
fn the_weakest_dimension_binds_the_report() {
    let mut payload = complete_payload();
    payload.declared_scope.expected_test_command = None;
    payload.source_class = CodingAgentSourceClass::IndependentlyObserved;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    let report = payload.coverage_report();
    assert_eq!(
        report.weakest_ceiling(),
        Some(CodingAgentClaimCeiling::IndependentlyConfirmed)
    );
    assert!(report.meets(CodingAgentClaimCeiling::ObservedInPath));

    // One unobserved dimension removes the ceiling entirely: a gap is not a weaker rung.
    payload.coverage.network = CodingAgentCoverageState::Absent;
    let report = payload.coverage_report();
    assert_eq!(report.weakest_ceiling(), None);
    assert!(!report.meets(CodingAgentClaimCeiling::Asserted));
}

#[test]
fn top_tier_vantage_that_did_not_watch_is_incomplete_not_clean() {
    for state in [
        CodingAgentCoverageState::Absent,
        CodingAgentCoverageState::Unavailable,
        CodingAgentCoverageState::Partial,
        CodingAgentCoverageState::SelfReported,
    ] {
        let got =
            coding_agent_dimension_conclusion(CodingAgentSourceClass::IndependentlyObserved, state);
        assert!(
            matches!(got, CodingAgentDimensionConclusion::Incomplete { .. }),
            "{state:?} under the strongest source class must not be Supported, got {got:?}"
        );
    }
}

#[test]
fn each_unobserved_state_keeps_its_own_reason() {
    let cases = [
        (
            CodingAgentCoverageState::Absent,
            CodingAgentCoverageGap::NotObserved,
        ),
        (
            CodingAgentCoverageState::Unavailable,
            CodingAgentCoverageGap::ObserverUnavailable,
        ),
        (
            CodingAgentCoverageState::SelfReported,
            CodingAgentCoverageGap::SelfReportedOnly,
        ),
        (
            CodingAgentCoverageState::Partial,
            CodingAgentCoverageGap::PartialOnly,
        ),
    ];
    for (state, want) in cases {
        assert_eq!(
            coding_agent_dimension_conclusion(CodingAgentSourceClass::BoundaryObserved, state),
            CodingAgentDimensionConclusion::Incomplete { gap: want },
            "{state:?} collapsed into another reason"
        );
    }
}

#[test]
fn observed_from_inside_the_subject_caps_at_the_bottom_two_rungs() {
    for (source, want) in [
        (
            CodingAgentSourceClass::ProducerReported,
            CodingAgentClaimCeiling::Asserted,
        ),
        (
            CodingAgentSourceClass::IssuerAttested,
            CodingAgentClaimCeiling::AssertedSigned,
        ),
    ] {
        assert_eq!(
            coding_agent_dimension_conclusion(source, CodingAgentCoverageState::Observed),
            CodingAgentDimensionConclusion::Supported { ceiling: want },
            "{source:?} watched, but it is not outside the subject"
        );
    }
}

#[test]
fn coverage_is_evaluated_before_source_class() {
    // The producer-reported case would be Supported{Asserted} if it had looked. It did not, and the
    // coverage reason must survive rather than being overwritten by the weaker source class.
    assert_eq!(
        coding_agent_dimension_conclusion(
            CodingAgentSourceClass::ProducerReported,
            CodingAgentCoverageState::Absent,
        ),
        CodingAgentDimensionConclusion::Incomplete {
            gap: CodingAgentCoverageGap::NotObserved
        },
    );
}

#[test]
fn undeclared_test_dimension_is_out_of_scope_not_a_gap() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::IndependentlyObserved;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    payload.coverage.test = CodingAgentCoverageState::Absent;

    payload.declared_scope.expected_test_command = None;
    let report = payload.coverage_report();
    assert!(
        report.test.is_none(),
        "a dimension nobody claimed is not a gap"
    );
    assert!(
        report.weakest_ceiling().is_some(),
        "unclaimed test must not sink the report"
    );
    assert!(report.gaps().is_empty());

    payload.declared_scope.expected_test_command = Some("cargo test".to_string());
    let report = payload.coverage_report();
    assert!(
        report.test.is_some(),
        "a declared test command puts the dimension in scope"
    );
    assert!(
        report.weakest_ceiling().is_none(),
        "declared-but-unobserved must sink the report"
    );
    assert_eq!(report.gaps().len(), 1);
    assert_eq!(report.gaps()[0].0, "test");
}

#[test]
fn sufficiency_requires_every_claimed_dimension_and_the_check_bites() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::BoundaryObserved;
    payload.declared_scope.expected_test_command = None;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    assert!(payload.coverage_report().weakest_ceiling().is_some());

    // One dimension at a time: each must be able to sink it on its own, or `is_sufficient`
    // is not actually reading that field.
    for (name, set) in [
        ("files", 0usize),
        ("commands", 1),
        ("network", 2),
        ("mcp_tools", 3),
    ] {
        let mut p = payload.clone();
        match set {
            0 => p.coverage.files = CodingAgentCoverageState::Absent,
            1 => p.coverage.commands = CodingAgentCoverageState::Absent,
            2 => p.coverage.network = CodingAgentCoverageState::Absent,
            _ => p.coverage.mcp_tools = CodingAgentCoverageState::Absent,
        }
        let report = p.coverage_report();
        assert!(
            report.weakest_ceiling().is_none(),
            "{name} unobserved did not sink the report"
        );
        assert_eq!(report.gaps().len(), 1, "{name}");
        assert_eq!(report.gaps()[0].0, name);
    }
}

#[test]
fn gaps_name_the_dimension_and_the_reason() {
    let mut payload = complete_payload();
    payload.source_class = CodingAgentSourceClass::IndependentlyObserved;
    payload.declared_scope.expected_test_command = None;
    payload.coverage = coverage_all(CodingAgentCoverageState::Observed);
    payload.coverage.network = CodingAgentCoverageState::Absent;
    payload.coverage.mcp_tools = CodingAgentCoverageState::Partial;

    let report = payload.coverage_report();
    let gaps = report.gaps();
    assert_eq!(gaps.len(), 2);
    assert_eq!(gaps[0].0, "network");
    assert_eq!(gaps[0].1, CodingAgentCoverageGap::NotObserved);
    assert_eq!(gaps[1].0, "mcp_tools");
    assert_eq!(gaps[1].1, CodingAgentCoverageGap::PartialOnly);
}
