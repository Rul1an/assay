use assay_evidence::{
    coding_agent_evidence_event, types::Payload, CodingAgentCoverage, CodingAgentCoverageState,
    CodingAgentDeclaredScope, CodingAgentEvidencePayload, CodingAgentNetworkPolicy,
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
