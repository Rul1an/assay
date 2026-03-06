use super::*;
use crate::UcpAdapter;

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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/adr026/ucp/v2026-01-23")
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
            | "actor"
            | "discovery"
            | "order"
            | "checkout"
            | "fulfillment"
            | "message"
            | "attributes"
    )
}

#[test]
fn protocol_metadata_uses_frozen_release_tag() {
    let adapter = UcpAdapter;
    let descriptor = adapter.adapter();
    let protocol = adapter.protocol();
    let capabilities = adapter.capabilities();

    assert_eq!(descriptor.adapter_id, ADAPTER_ID);
    assert!(!descriptor.adapter_version.is_empty());
    assert_eq!(protocol.name, PROTOCOL_NAME);
    assert_eq!(protocol.spec_version, PROTOCOL_VERSION);
    assert_eq!(capabilities.supported_spec_versions, vec![PROTOCOL_VERSION]);
}

#[test]
fn strict_discovery_fixture_emits_deterministic_event() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_happy_discovery_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some(PROTOCOL_VERSION),
    };

    let first = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict discovery fixture should convert");
    let second = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict discovery fixture should convert deterministically");

    assert_eq!(first.events.len(), 1);
    assert_eq!(
        first.events[0].type_,
        "assay.adapter.ucp.discovery.requested"
    );
    assert_eq!(first.events[0].subject.as_deref(), Some("discovery-100"));
    assert_eq!(first.lossiness.lossiness_level, LossinessLevel::None);
    assert_eq!(
        digest_canonical_json(&first),
        digest_canonical_json(&second)
    );
    assert_eq!(
        first.events[0].payload["adapter_id"],
        Value::String(ADAPTER_ID.to_string())
    );
    assert_eq!(
        first.events[0].payload["protocol_name"],
        Value::String(PROTOCOL_NAME.to_string())
    );
    assert_eq!(
        first.events[0].payload["discovery"]["query"],
        Value::String("running shoes".to_string())
    );
}

#[test]
fn strict_order_fixture_maps_expected_event() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_happy_order_requested.json");
    let batch = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("strict order fixture should convert");

    assert_eq!(batch.events[0].type_, "assay.adapter.ucp.order.requested");
    assert_eq!(batch.events[0].subject.as_deref(), Some("order-200"));
    assert_eq!(batch.lossiness.lossiness_level, LossinessLevel::None);
}

#[test]
fn strict_checkout_fixture_maps_expected_event() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_happy_checkout_updated.json");
    let batch = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("strict checkout fixture should convert");

    assert_eq!(batch.events[0].type_, "assay.adapter.ucp.checkout.updated");
    assert_eq!(batch.events[0].subject.as_deref(), Some("checkout-300"));
}

#[test]
fn strict_fulfillment_fixture_maps_expected_event() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_happy_fulfillment_updated.json");
    let batch = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect("strict fulfillment fixture should convert");

    assert_eq!(
        batch.events[0].type_,
        "assay.adapter.ucp.fulfillment.updated"
    );
    assert_eq!(batch.events[0].subject.as_deref(), Some("fulfillment-400"));
}

#[test]
fn strict_key_order_independent_event_digest_keeps_raw_hash_bytes_exact() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload_a = br#"{
      "protocol":"ucp",
      "version":"v2026-01-23",
      "event_type":"order.requested",
      "timestamp":"2026-02-28T10:05:00Z",
      "actor":{"id":"merchant-agent-7","role":"merchant_agent"},
      "order":{"id":"order-200","status":"requested","currency":"USD","total":"42.00"},
      "attributes":{"tenant":"acme","channel":"web"}
    }"#;
    let payload_b = br#"{
      "version":"v2026-01-23",
      "protocol":"ucp",
      "timestamp":"2026-02-28T10:05:00Z",
      "event_type":"order.requested",
      "order":{"total":"42.00","currency":"USD","status":"requested","id":"order-200"},
      "actor":{"role":"merchant_agent","id":"merchant-agent-7"},
      "attributes":{"channel":"web","tenant":"acme"}
    }"#;

    let first = adapter
        .convert(
            AdapterInput {
                payload: payload_a,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
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
                protocol_version: Some(PROTOCOL_VERSION),
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
fn strict_missing_order_id_fails_with_measurement_error() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_negative_missing_order_id.json");
    let err = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect_err("strict missing order id must fail");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn lenient_missing_order_id_substitutes_unknown_order() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_negative_missing_order_id.json");
    let batch = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions {
                mode: ConvertMode::Lenient,
                max_payload_bytes: Some(8_192),
                max_json_depth: None,
                max_array_length: None,
            },
            &writer,
        )
        .expect("lenient missing order id should substitute unknown order");

    assert_eq!(batch.events[0].type_, "assay.adapter.ucp.order.requested");
    assert_eq!(batch.events[0].subject.as_deref(), Some("unknown-order"));
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert!(batch.lossiness.raw_payload_ref.is_some());
}

#[test]
fn lenient_invalid_event_type_emits_generic_message_event_and_lossiness() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_negative_invalid_event_type.json");
    let batch = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions {
                mode: ConvertMode::Lenient,
                max_payload_bytes: Some(8_192),
                max_json_depth: None,
                max_array_length: None,
            },
            &writer,
        )
        .expect("lenient invalid event_type should emit generic event");

    assert_eq!(batch.events[0].type_, "assay.adapter.ucp.message");
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
}

#[test]
fn malformed_json_fails_in_all_modes() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_negative_malformed.json");
    let err = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
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
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = fixture("ucp_happy_order_requested.json");
    let err = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
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
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = [0xff, 0xfe, 0xfd];
    let err = adapter
        .convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        )
        .expect_err("invalid UTF-8 must fail measurement contract");
    assert_eq!(err.kind, AdapterErrorKind::Measurement);
}

#[test]
fn excessive_json_depth_fails_measurement_contract() {
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"ucp",
      "version":"v2026-01-23",
      "event_type":"order.requested",
      "timestamp":"2026-02-28T10:05:00Z",
      "actor":{"id":"merchant-agent-7","role":"merchant_agent"},
      "order":{"id":"order-200","status":"requested"},
      "attributes":{"nested":{"deeper":{"value":"x"}}}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
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
    let adapter = UcpAdapter;
    let writer = TestWriter;
    let payload = br#"{
      "protocol":"ucp",
      "version":"v2026-01-23",
      "event_type":"discovery.requested",
      "timestamp":"2026-02-28T10:00:00Z",
      "actor":{"id":"buyer-agent-1","role":"buyer_agent"},
      "discovery":{"id":"discovery-100","query":"running shoes"},
      "attributes":{"facets":["a","b","c","d"]}
    }"#;

    let err = adapter
        .convert(
            AdapterInput {
                payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
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
        let mut packet: Value = serde_json::from_slice(&fixture("ucp_happy_order_requested.json")).unwrap();
        let object = packet.as_object_mut().unwrap();
        let mut inserted = 0u32;

        for (key, value) in extras {
            prop_assume!(!reserved_key(&key));
            object.insert(key, Value::String(value));
            inserted += 1;
        }

        let payload = serde_json::to_vec(&packet).unwrap();
        let adapter = UcpAdapter;
        let writer = TestWriter;
        let batch = adapter.convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some(PROTOCOL_VERSION),
            },
            &ConvertOptions::default(),
            &writer,
        ).unwrap();

        prop_assert!(batch.lossiness.unmapped_fields_count >= inserted);
    }
}
