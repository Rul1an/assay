use super::*;
use crate::A2aAdapter;

use assay_adapter_api::{
    digest_canonical_json, AdapterErrorKind, AdapterInput, AdapterResult, AttachmentWriter,
    ConvertMode, ConvertOptions, LossinessLevel, ProtocolAdapter, RawPayloadRef,
};
use proptest::prelude::*;
use serde_json::Value;
use sha2::Digest;
use std::{fs, path::PathBuf};

struct TestWriter;

impl AttachmentWriter for TestWriter {
    fn write_raw_payload(&self, payload: &[u8], media_type: &str) -> AdapterResult<RawPayloadRef> {
        Ok(RawPayloadRef {
            sha256: hex::encode(sha2::Sha256::digest(payload)),
            size_bytes: payload.len() as u64,
            media_type: media_type.to_string(),
        })
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ci/fixtures/adr026/a2a/v0.2")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixture_dir().join(name)).expect("fixture must exist")
}

fn reserved_key(key: &str) -> bool {
    matches!(
        key,
        "protocol"
            | "version"
            | "event_type"
            | "timestamp"
            | "agent"
            | "task"
            | "artifact"
            | "message"
            | "attributes"
    )
}

fn assert_discovery_v1_defaults(payload: &Value) {
    let d = &payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(false));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(false));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
}

fn assert_handoff_v1_defaults(payload: &Value) {
    let h = &payload["handoff"];
    assert_eq!(h["visible"], Value::Bool(false));
    assert_eq!(h["source_kind"], Value::String("unknown".to_string()));
    assert_eq!(h["task_ref_visible"], Value::Bool(false));
    assert_eq!(h["message_ref_visible"], Value::Bool(false));
}

/// Golden digests over `payload.discovery` via `digest_canonical_json` (sorted keys).
/// If `discovery` shape or types change intentionally, update hashes and this comment.
const G4_DISCOVERY_DIGEST_DEFAULT: &str =
    "26b4d9c0105f4cc26d4b413e7b6b27effe5829f9f319a60b91ca490fd7776a13";
const G4_DISCOVERY_DIGEST_AGENT_CARD_ATTR: &str =
    "93f5c26d149e7400d38104c4479f332df4df23df0d1f4d25aef252aac87b9769";
const G4_DISCOVERY_DIGEST_BOTH_FLAGS: &str =
    "9d0f24e430e00ee3ec1bc595cb59e6e7d7d5b12c0c90e102ea4d26ad3890e665";
/// Extended visibility only (`agent_card_source_kind` stays `unknown`).
const G4_DISCOVERY_DIGEST_EXTENDED_ONLY: &str =
    "13e23c6783de838b52ca92d787569bccd3cadc0f8900f1bf76b42262959f77ba";
const K1_HANDOFF_DIGEST_DEFAULT: &str =
    "60e992b4881c03d816cd94929856d8c8cade113f62273d42a8a75412533a294a";
const K1_HANDOFF_DIGEST_TYPED_POSITIVE: &str =
    "e478af7359a254678c90b5eb2737d63f79c6d667a2b5c4bc323442f07d09d33b";
const K1_HANDOFF_DIGEST_LENIENT_PARTIAL: &str =
    "0be260743587b9594018a4ab7809560157be088be0372a8ae7c7faa6a744effe";

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

#[test]
fn g4_n1_non_allowlisted_attributes_only_yields_unknown_kind() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_n2_assay_g4_wrong_shape_does_not_promote() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .get_mut("attributes")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("assay_g4".to_string(), Value::String("bad".to_string()));
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_n3_unmapped_top_level_fields_alone_do_not_affect_discovery() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .insert("extra_top_level".to_string(), Value::Number(1.into()));
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_attributes_driven_agent_card_sets_kind_attributes() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_agent_card.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(true));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("attributes".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(false));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(
        digest_canonical_json(d),
        G4_DISCOVERY_DIGEST_AGENT_CARD_ATTR
    );
}

/// When `discovery` is non-default, full `AdapterBatch` digests must still be byte-stable across
/// repeated conversion (not only `payload["discovery"]` golden hashes).
#[test]
fn g4_non_default_discovery_full_batch_digest_is_deterministic() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_agent_card.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
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
fn g4_both_visibility_flags_true_fixture_shows_independence() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_both_visible.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(true));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("attributes".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(true));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(digest_canonical_json(d), G4_DISCOVERY_DIGEST_BOTH_FLAGS);
}

#[test]
fn g4_extended_access_visible_positive_fixture() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_extended.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(false));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(true));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(digest_canonical_json(d), G4_DISCOVERY_DIGEST_EXTENDED_ONLY);
}

#[test]
fn g4_n5_strict_and_lenient_same_discovery_without_assay_g4() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_task_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };
    let strict = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict");
    let lenient = adapter
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
        .expect("lenient");
    assert_eq!(
        strict.events[0].payload["discovery"],
        lenient.events[0].payload["discovery"]
    );
    assert_discovery_v1_defaults(&strict.events[0].payload);
}

#[test]
fn g4_missing_agent_card_object_does_not_promote() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .get_mut("attributes")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(
            "assay_g4".to_string(),
            serde_json::json!({ "priority": "nested" }),
        );
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_eq!(
        batch.events[0].payload["discovery"]["agent_card_visible"],
        false
    );
    assert_eq!(
        batch.events[0].payload["discovery"]["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn strict_unknown_top_level_fields_account_for_lossiness(
        extras in proptest::collection::btree_map("[a-z_]{1,12}", "[a-z0-9_-]{0,12}", 1..5)
    ) {
        let mut packet: Value = serde_json::from_slice(&fixture("a2a_happy_task_requested.json")).unwrap();
        let object = packet.as_object_mut().unwrap();
        let mut inserted = 0u32;

        for (key, value) in extras {
            prop_assume!(!reserved_key(&key));
            object.insert(key, Value::String(value));
            inserted += 1;
        }

        let payload = serde_json::to_vec(&packet).unwrap();
        let adapter = A2aAdapter;
        let writer = TestWriter;
        let batch = adapter.convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        ).unwrap();

        prop_assert!(batch.lossiness.unmapped_fields_count >= inserted);
    }
}
