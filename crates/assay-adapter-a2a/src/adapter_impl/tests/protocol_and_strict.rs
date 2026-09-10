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
    assert_eq!(protocol.schema_id.as_deref(), Some(PROFILE_NAME));
    assert_eq!(
        capabilities.supported_spec_versions,
        vec!["0.2".to_string(), "0.2.0".to_string(), "0.3.1".to_string()]
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

#[test]
fn strict_version_0_99_is_rejected_before_mapping() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol": "a2a",
      "version": "0.99",
      "event_type": "task.requested",
      "agent": {"id": "agent-1"},
      "task": {"id": "task-1"}
    }"#;
    let input = AdapterInput {
        payload,
        media_type: "application/json",
        protocol_version: None,
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("version 0.99 must be rejected");
    assert_eq!(err.kind, AdapterErrorKind::UnsupportedProtocolVersion);
    assert!(
        err.message.contains("0.99"),
        "error message should cite rejected version: {}",
        err.message
    );
}

#[test]
fn strict_version_1_0_is_rejected_before_mapping() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol": "a2a",
      "version": "1.0",
      "event_type": "task.requested",
      "agent": {"id": "agent-1"},
      "task": {"id": "task-1"}
    }"#;
    let input = AdapterInput {
        payload,
        media_type: "application/json",
        protocol_version: None,
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("version 1.0 must be rejected");
    assert_eq!(err.kind, AdapterErrorKind::UnsupportedProtocolVersion);
}

#[test]
fn strict_undeclared_version_0_4_is_rejected_before_mapping() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol": "a2a",
      "version": "0.4",
      "event_type": "task.requested",
      "agent": {"id": "agent-1"},
      "task": {"id": "task-1"}
    }"#;
    let input = AdapterInput {
        payload,
        media_type: "application/json",
        protocol_version: None,
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("undeclared version 0.4 must be rejected");
    assert_eq!(err.kind, AdapterErrorKind::UnsupportedProtocolVersion);
}

#[test]
fn declared_supported_versions_match_fixture_inventory_exactly() {
    let adapter = A2aAdapter;
    let capabilities = adapter.capabilities();
    let protocol = adapter.protocol();

    assert_eq!(
        protocol.schema_id.as_deref(),
        Some("assay.adapter.a2a.legacy-projection.v0"),
        "protocol descriptor must report the explicit Assay-owned profile name"
    );

    // Extract all versions present in real fixtures under scripts/ci/fixtures/adr026/a2a
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ci/fixtures/adr026/a2a");
    let mut fixture_versions = std::collections::BTreeSet::new();

    fn collect_versions(dir: &std::path::Path, versions: &mut std::collections::BTreeSet<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_versions(&path, versions);
                } else if path.extension().is_some_and(|ext| ext == "json") {
                    let content = fs::read_to_string(&path).expect("fixture must be readable");
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        if let Some(v) = val.get("version").and_then(|v| v.as_str()) {
                            versions.insert(v.to_string());
                        }
                    } else {
                        // For malformed json fixtures, extract version field via quote boundary
                        for line in content.lines() {
                            if let Some(idx) = line.find("\"version\"") {
                                let rest = &line[idx + 9..];
                                if let Some(colon) = rest.find(':') {
                                    let after_colon = &rest[colon + 1..];
                                    if let Some(q1) = after_colon.find('"') {
                                        let after_q1 = &after_colon[q1 + 1..];
                                        if let Some(q2) = after_q1.find('"') {
                                            let v = &after_q1[..q2];
                                            if !v.is_empty() {
                                                versions.insert(v.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    collect_versions(&root, &mut fixture_versions);
    assert!(
        !fixture_versions.is_empty(),
        "must find fixture versions under {}",
        root.display()
    );

    let declared_versions: std::collections::BTreeSet<String> =
        capabilities.supported_spec_versions.into_iter().collect();

    assert_eq!(
        declared_versions, fixture_versions,
        "declared supported_spec_versions and fixture versions must match exactly"
    );
}

#[test]
fn strict_profile_mismatch_is_rejected() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol": "a2a",
      "profile": "unsupported.profile.v1",
      "version": "0.2.0",
      "event_type": "task.requested",
      "agent": {"id": "agent-1"},
      "task": {"id": "task-1"}
    }"#;
    let input = AdapterInput {
        payload,
        media_type: "application/json",
        protocol_version: None,
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("mismatched profile must be rejected");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}
