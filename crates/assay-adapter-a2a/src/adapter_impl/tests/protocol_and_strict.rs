use super::*;
#[test]
fn protocol_metadata_uses_exact_version_and_range_capability() {
    let adapter = A2aAdapter;
    let descriptor = adapter.adapter();
    let protocol = adapter.protocol();
    let capabilities = adapter.capabilities();

    assert_eq!(descriptor.adapter_id, ADAPTER_ID);
    assert!(!descriptor.adapter_version.is_empty());
    assert_eq!(protocol.spec_version, "0.2.0");
    assert_eq!(
        capabilities.supported_spec_versions,
        vec![">=0.2 <1.0".to_string()]
    );
}

#[test]
fn strict_agent_capabilities_fixture_emits_deterministic_event() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };

    let first = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict happy fixture should convert");
    let second = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict happy fixture should convert deterministically");

    assert_eq!(first.events.len(), 1);
    assert_eq!(
        first.events[0].type_,
        "assay.adapter.a2a.agent.capabilities"
    );
    assert_eq!(first.lossiness.lossiness_level, LossinessLevel::None);
    assert_eq!(
        digest_canonical_json(&first),
        digest_canonical_json(&second)
    );
    assert_eq!(
        first.events[0].payload["agent"]["capabilities"],
        serde_json::json!(["agent.describe", "artifacts.share", "tasks.update"])
    );
    assert_discovery_v1_defaults(&first.events[0].payload);
    assert_handoff_v1_defaults(&first.events[0].payload);
    assert_eq!(
        digest_canonical_json(&first.events[0].payload["discovery"]),
        G4_DISCOVERY_DIGEST_DEFAULT
    );
    assert_eq!(
        digest_canonical_json(&first.events[0].payload["handoff"]),
        K1_HANDOFF_DIGEST_DEFAULT
    );
}

#[test]
fn strict_key_order_independent_event_digest_keeps_raw_hash_bytes_exact() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload_a = br#"{
      "protocol":"a2a",
      "version":"0.2.0",
      "event_type":"task.requested",
      "timestamp":"2026-02-27T11:05:00Z",
      "agent":{"id":"agent-7","name":"Agent Seven","role":"planner","capabilities":["tasks.update","agent.describe"]},
      "task":{"id":"task-xyz","status":"queued","kind":"analysis"},
      "attributes":{"priority":"high","tenant":"acme"}
    }"#;
    let payload_b = br#"{
      "version":"0.2.0",
      "protocol":"a2a",
      "timestamp":"2026-02-27T11:05:00Z",
      "event_type":"task.requested",
      "task":{"kind":"analysis","status":"queued","id":"task-xyz"},
      "agent":{"role":"planner","name":"Agent Seven","id":"agent-7","capabilities":["agent.describe","tasks.update"]},
      "attributes":{"tenant":"acme","priority":"high"}
    }"#;

    let first = adapter
        .convert(
            AdapterInput {
                payload: payload_a,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("first payload should convert");
    let second = adapter
        .convert(
            AdapterInput {
                payload: payload_b,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("second payload should convert");

    assert_eq!(
        digest_canonical_json(&first.events[0].payload),
        digest_canonical_json(&second.events[0].payload)
    );
    assert_ne!(
        first
            .lossiness
            .raw_payload_ref
            .as_ref()
            .map(|raw| raw.sha256.clone()),
        second
            .lossiness
            .raw_payload_ref
            .as_ref()
            .map(|raw| raw.sha256.clone())
    );
}

#[test]
fn strict_task_requested_fixture_maps_expected_event() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_task_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict task fixture should convert");

    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.task.requested");
    assert_eq!(batch.events[0].subject.as_deref(), Some("task-123"));
    assert_eq!(batch.lossiness.lossiness_level, LossinessLevel::None);
    assert_eq!(
        batch.events[0].payload["adapter_id"],
        Value::String(ADAPTER_ID.to_string())
    );
    assert_eq!(
        batch.events[0].payload["adapter_version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );
    assert_eq!(
        batch.events[0].payload["protocol_name"],
        Value::String(PROTOCOL_NAME.to_string())
    );
    let h = &batch.events[0].payload["handoff"];
    assert_eq!(h["visible"], Value::Bool(true));
    assert_eq!(h["source_kind"], Value::String("typed_payload".to_string()));
    assert_eq!(h["task_ref_visible"], Value::Bool(true));
    assert_eq!(h["message_ref_visible"], Value::Bool(true));
    assert_eq!(digest_canonical_json(h), K1_HANDOFF_DIGEST_TYPED_POSITIVE);
}

#[test]
fn strict_artifact_shared_fixture_maps_expected_event() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_artifact_shared.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.3.1"),
    };

    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict artifact fixture should convert");

    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.artifact.shared");
    assert_eq!(batch.events[0].subject.as_deref(), Some("artifact-7"));
    assert_handoff_v1_defaults(&batch.events[0].payload);
    assert_eq!(
        digest_canonical_json(&batch.events[0].payload["handoff"]),
        K1_HANDOFF_DIGEST_DEFAULT
    );
}

#[test]
fn strict_missing_task_id_fails_with_measurement_error() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_negative_missing_task_id.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("strict missing task id must fail");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn lenient_missing_task_id_substitutes_unknown_task() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_negative_missing_task_id.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let batch = adapter
        .convert(
            input,
            &ConvertOptions {
                mode: ConvertMode::Lenient,
                max_payload_bytes: Some(8_192),
                max_json_depth: None,
                max_array_length: None,
            },
            &writer,
        )
        .expect("lenient missing task id should substitute unknown task");

    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.task.requested");
    assert_eq!(batch.events[0].subject.as_deref(), Some("unknown-task"));
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert!(batch.lossiness.raw_payload_ref.is_some());
    let h = &batch.events[0].payload["handoff"];
    assert_eq!(h["visible"], Value::Bool(true));
    assert_eq!(h["source_kind"], Value::String("typed_payload".to_string()));
    assert_eq!(h["task_ref_visible"], Value::Bool(false));
    assert_eq!(h["message_ref_visible"], Value::Bool(true));
    assert_eq!(digest_canonical_json(h), K1_HANDOFF_DIGEST_LENIENT_PARTIAL);
}
