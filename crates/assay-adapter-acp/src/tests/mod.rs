use super::*;
use assay_adapter_api::{
    digest_canonical_json, AdapterErrorKind, AttachmentWriter, ConvertMode, ConvertOptions,
    LossinessLevel, RawPayloadRef,
};
use proptest::prelude::*;
use serde_json::Value;
use sha2::Digest;
use std::fs;
use std::path::PathBuf;

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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ci/fixtures/adr026/acp/v2.11.0")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixture_dir().join(name)).expect("fixture must exist")
}

fn reserved_key(key: &str) -> bool {
    matches!(
        key,
        "protocol"
            | "version"
            | "packet_id"
            | "event_type"
            | "timestamp"
            | "actor"
            | "intent"
            | "attributes"
    )
}

#[test]
fn strict_happy_fixture_emits_deterministic_event() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_happy_intent_created.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
    };

    let first = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict happy fixture should convert");
    let second = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict happy fixture should convert deterministically");

    assert_eq!(first.events.len(), 1);
    assert_eq!(first.lossiness.lossiness_level, LossinessLevel::None);
    assert_eq!(
        digest_canonical_json(&first),
        digest_canonical_json(&second)
    );
    assert_eq!(first.events[0].type_, "assay.adapter.acp.intent.created");
    assert_eq!(
        first.events[0].payload["adapter_id"],
        Value::String(ADAPTER_ID.to_string())
    );
    assert_eq!(
        first.events[0].payload["adapter_version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );
    assert_eq!(
        first.events[0].payload["protocol_name"],
        Value::String(PROTOCOL_NAME.to_string())
    );
    assert_eq!(
        first.events[0].payload["attributes"]["merchant_id"],
        Value::String("merchant-42".to_string())
    );
}

#[test]
fn strict_checkout_fixture_preserves_attributes_without_lossiness() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_happy_checkout_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
    };

    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict checkout fixture should convert");

    assert_eq!(batch.lossiness.lossiness_level, LossinessLevel::None);
    assert_eq!(
        batch.events[0].payload["attributes"],
        serde_json::json!({
            "amount": "42.00",
            "currency": "USD"
        })
    );
}

#[test]
fn strict_attribute_order_normalizes_payload_but_keeps_raw_byte_hash_boundary() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload_a = br#"{
      "protocol":"acp",
      "version":"2.11.0",
      "packet_id":"pkt-order-1",
      "event_type":"checkout.requested",
      "timestamp":"2026-02-27T10:05:00Z",
      "actor":{"id":"agent-buyer-2","role":"buyer_agent"},
      "intent":{"id":"intent-2001","kind":"checkout"},
      "attributes":{"currency":"USD","amount":"42.00"}
    }"#;
    let payload_b = br#"{
      "version":"2.11.0",
      "protocol":"acp",
      "packet_id":"pkt-order-1",
      "timestamp":"2026-02-27T10:05:00Z",
      "event_type":"checkout.requested",
      "intent":{"kind":"checkout","id":"intent-2001"},
      "actor":{"role":"buyer_agent","id":"agent-buyer-2"},
      "attributes":{"amount":"42.00","currency":"USD"}
    }"#;

    let first = adapter
        .convert(
            AdapterInput {
                payload: payload_a,
                media_type: "application/json",
                protocol_version: Some(SPEC_VERSION),
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
                protocol_version: Some(SPEC_VERSION),
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
fn strict_missing_required_field_fails_with_measurement_error() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_negative_missing_packet_id.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("strict missing field must fail");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn lenient_invalid_event_type_emits_generic_event_and_lossiness() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_negative_invalid_event_type.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
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
        .expect("lenient invalid event_type should still emit generic packet event");

    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].type_, "assay.adapter.acp.packet");
    assert!(matches!(
        batch.lossiness.lossiness_level,
        LossinessLevel::Low | LossinessLevel::High
    ));
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert!(batch.lossiness.raw_payload_ref.is_some());
    assert_eq!(
        batch.events[0].payload["adapter_id"],
        Value::String(ADAPTER_ID.to_string())
    );
    assert_eq!(
        batch.events[0].payload["adapter_version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );
}

#[test]
fn malformed_json_fails_in_all_modes() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_negative_malformed.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
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
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = fixture("acp_happy_checkout_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
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
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = [0xff, 0xfe, 0xfd];
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(SPEC_VERSION),
    };

    let err = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect_err("invalid UTF-8 must fail measurement contract");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn excessive_json_depth_fails_measurement_contract() {
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"acp",
      "version":"2.11.0",
      "packet_id":"pkt-depth-1",
      "event_type":"intent.created",
      "timestamp":"2026-02-27T10:00:00Z",
      "actor":{"id":"agent-1","role":"buyer_agent"},
      "intent":{"id":"intent-1","kind":"checkout"},
      "attributes":{"nested":{"deeper":{"value":"x"}}}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some(SPEC_VERSION),
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
    let adapter = AcpAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"acp",
      "version":"2.11.0",
      "packet_id":"pkt-array-1",
      "event_type":"checkout.requested",
      "timestamp":"2026-02-27T10:05:00Z",
      "actor":{"id":"agent-2","role":"buyer_agent"},
      "intent":{"id":"intent-2","kind":"checkout"},
      "attributes":{"items":[1,2,3,4]}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some(SPEC_VERSION),
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn strict_unknown_top_level_fields_account_for_lossiness(
        extras in proptest::collection::btree_map("[a-z_]{1,12}", "[a-z0-9_-]{0,12}", 1..5)
    ) {
        let mut packet: Value = serde_json::from_slice(&fixture("acp_happy_intent_created.json")).unwrap();
        let object = packet.as_object_mut().unwrap();
        let mut inserted = 0u32;

        for (key, value) in extras {
            prop_assume!(!reserved_key(&key));
            object.insert(key, Value::String(value));
            inserted += 1;
        }

        let payload = serde_json::to_vec(&packet).unwrap();
        let adapter = AcpAdapter;
        let writer = TestWriter;
        let batch = adapter
            .convert(
                AdapterInput {
                    payload: &payload,
                    media_type: "application/json",
                    protocol_version: Some(SPEC_VERSION),
                },
                &ConvertOptions::default(),
                &writer,
            )
            .unwrap();

        prop_assert!(batch.lossiness.unmapped_fields_count >= inserted);
    }
}
