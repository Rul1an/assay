use super::*;
#[test]
fn lenient_invalid_event_type_emits_generic_message_event_and_lossiness() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_negative_invalid_event_type.json");
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
        .expect("lenient invalid event_type should emit generic event");

    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.message");
    assert!(matches!(
        batch.lossiness.lossiness_level,
        LossinessLevel::Low | LossinessLevel::High
    ));
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert_eq!(
        batch.events[0].payload["adapter_id"],
        Value::String(ADAPTER_ID.to_string())
    );
    assert_eq!(
        batch.events[0].payload["adapter_version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );
    assert_handoff_v1_defaults(&batch.events[0].payload);
    assert_eq!(
        digest_canonical_json(&batch.events[0].payload["handoff"]),
        K1_HANDOFF_DIGEST_DEFAULT
    );
}

#[test]
fn k1_typed_positive_full_batch_digest_is_deterministic() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_task_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };
    let first = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let second = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_eq!(
        digest_canonical_json(&first),
        digest_canonical_json(&second)
    );
}

#[test]
fn k1_task_updated_delegation_does_not_promote_handoff_in_v1() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"a2a",
      "version":"0.2.0",
      "event_type":"task.updated",
      "timestamp":"2026-02-27T11:05:00Z",
      "agent":{"id":"agent://worker"},
      "task":{"id":"task-999","status":"running","kind":"delegation"},
      "message":{"id":"msg-update","role":"assistant"}
    }"#;

    let batch = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("task.updated should still convert");

    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.task.updated");
    assert_handoff_v1_defaults(&batch.events[0].payload);
    assert_eq!(
        digest_canonical_json(&batch.events[0].payload["handoff"]),
        K1_HANDOFF_DIGEST_DEFAULT
    );
}

#[test]
fn k1_task_requested_non_delegation_does_not_promote_handoff_in_v1() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"a2a",
      "version":"0.2.0",
      "event_type":"task.requested",
      "timestamp":"2026-02-27T11:10:00Z",
      "agent":{"id":"agent://worker"},
      "task":{"id":"task-1000","status":"requested","kind":"analysis"},
      "message":{"id":"msg-analysis","role":"assistant"}
    }"#;

    let batch = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("non-delegation task.requested should still convert");

    assert_eq!(batch.events[0].type_, "assay.adapter.a2a.task.requested");
    assert_handoff_v1_defaults(&batch.events[0].payload);
    assert_eq!(
        digest_canonical_json(&batch.events[0].payload["handoff"]),
        K1_HANDOFF_DIGEST_DEFAULT
    );
}

#[test]
fn malformed_json_fails_in_all_modes() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_negative_malformed.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let err = adapter
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
        .expect_err("malformed JSON must fail even in lenient mode");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn oversized_payload_fails_measurement_contract() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_task_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let err = adapter
        .convert(
            input,
            &ConvertOptions {
                mode: ConvertMode::Strict,
                max_payload_bytes: Some(32),
                max_json_depth: None,
                max_array_length: None,
            },
            &writer,
        )
        .expect_err("oversized payload must fail measurement contract");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn invalid_utf8_payload_fails_measurement_contract() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = [0xff, 0xfe, 0xfd];
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("invalid UTF-8 must fail measurement contract");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn excessive_json_depth_fails_measurement_contract() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"a2a",
      "version":"0.2.0",
      "event_type":"task.requested",
      "timestamp":"2026-02-27T11:05:00Z",
      "agent":{"id":"agent-7","name":"Agent Seven","role":"planner","capabilities":["tasks.update"]},
      "task":{"id":"task-xyz","status":"queued","kind":"analysis"},
      "attributes":{"nested":{"deeper":{"value":"x"}}}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions {
                mode: ConvertMode::Strict,
                max_payload_bytes: Some(8_192),
                max_json_depth: Some(4),
                max_array_length: None,
            },
            &writer,
        )
        .expect_err("deeply nested payload must fail");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
    assert!(err.message.contains("max_json_depth"));
}

#[test]
fn excessive_array_length_fails_measurement_contract() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"a2a",
      "version":"0.2.0",
      "event_type":"agent.capabilities",
      "timestamp":"2026-02-27T11:05:00Z",
      "agent":{"id":"agent-7","name":"Agent Seven","role":"planner","capabilities":["a","b","c","d"]},
      "attributes":{"items":[1,2,3,4]}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions {
                mode: ConvertMode::Strict,
                max_payload_bytes: Some(8_192),
                max_json_depth: None,
                max_array_length: Some(3),
            },
            &writer,
        )
        .expect_err("oversized array must fail");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
    assert!(err.message.contains("max_array_length"));
}
