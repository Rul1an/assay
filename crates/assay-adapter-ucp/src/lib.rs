//! UCP adapter MVP for translating a governance-relevant subset of UCP packets into canonical
//! Assay evidence events.

use assay_adapter_api::{
    validate_json_shape, AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterError,
    AdapterErrorKind, AdapterInput, AdapterResult, AttachmentWriter, ConvertMode, ConvertOptions,
    LossinessLevel, LossinessReport, ProtocolAdapter, ProtocolDescriptor,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Map, Value};

const PROTOCOL_NAME: &str = "ucp";
const PROTOCOL_VERSION: &str = "v2026-01-23";
const SUPPORTED_RELEASE_LINE: &str = "v2026-01-23";
const SCHEMA_ID: &str = "ucp.packet.v2026_01_23";
const SPEC_URL: &str = "https://github.com/google-agentic-commerce/ucp";
const DEFAULT_TIME_SECS: i64 = 1_700_300_000;
const ADAPTER_ID: &str = "assay-adapter-ucp";

/// UCP adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct UcpAdapter;

impl ProtocolAdapter for UcpAdapter {
    fn adapter(&self) -> AdapterDescriptor {
        AdapterDescriptor {
            adapter_id: ADAPTER_ID,
            adapter_version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: PROTOCOL_VERSION.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supported_event_types: vec![
                "assay.adapter.ucp.discovery.requested".to_string(),
                "assay.adapter.ucp.order.requested".to_string(),
                "assay.adapter.ucp.checkout.updated".to_string(),
                "assay.adapter.ucp.fulfillment.updated".to_string(),
                "assay.adapter.ucp.message".to_string(),
            ],
            supported_spec_versions: vec![SUPPORTED_RELEASE_LINE.to_string()],
            supports_strict: true,
            supports_lenient: true,
        }
    }

    fn convert(
        &self,
        input: AdapterInput<'_>,
        options: &ConvertOptions,
        attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch> {
        if let Some(limit) = options.max_payload_bytes {
            if input.payload.len() as u64 > limit {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    format!("payload exceeds max_payload_bytes ({limit})"),
                ));
            }
        }

        let raw_ref = attachments.write_raw_payload(input.payload, input.media_type)?;
        let packet = parse_packet(input.payload)?;
        validate_json_shape(&packet, options.max_json_depth, options.max_array_length)?;
        validate_protocol(&packet)?;
        let version = observed_version(&packet, input.protocol_version)?;
        validate_supported_version(&version)?;

        let mut notes = Vec::new();
        let mut unmapped_fields_count = count_unmapped_top_level_fields(&packet);

        let event_type = string_field(&packet, "event_type");
        let actor_id = nested_string_field(&packet, &["actor", "id"]);
        let mapped_event_type = map_event_type(event_type.as_deref());
        let discovery_id = nested_string_field(&packet, &["discovery", "id"]);
        let order_id = nested_string_field(&packet, &["order", "id"]);
        let checkout_id = nested_string_field(&packet, &["checkout", "id"]);
        let fulfillment_id = nested_string_field(&packet, &["fulfillment", "id"]);
        let message_id = nested_string_field(&packet, &["message", "id"]);

        if matches!(options.mode, ConvertMode::Strict) {
            if actor_id.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: actor.id",
                ));
            }

            match mapped_event_type {
                Some("assay.adapter.ucp.discovery.requested") if discovery_id.is_none() => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        "missing required field: discovery.id",
                    ));
                }
                Some("assay.adapter.ucp.order.requested") if order_id.is_none() => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        "missing required field: order.id",
                    ));
                }
                Some("assay.adapter.ucp.checkout.updated") if checkout_id.is_none() => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        "missing required field: checkout.id",
                    ));
                }
                Some("assay.adapter.ucp.fulfillment.updated") if fulfillment_id.is_none() => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        "missing required field: fulfillment.id",
                    ));
                }
                None => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        "unsupported event_type in strict mode",
                    ));
                }
                _ => {}
            }
        }

        let actor_id = actor_id.unwrap_or_else(|| {
            notes.push("missing actor.id -> substituted unknown-actor".to_string());
            unmapped_fields_count += 1;
            "unknown-actor".to_string()
        });

        let mapped_event_type = match mapped_event_type {
            Some(name) => name,
            None => {
                notes.push(
                    "unsupported event_type -> emitted generic UCP message event".to_string(),
                );
                unmapped_fields_count += 1;
                "assay.adapter.ucp.message"
            }
        };

        let discovery_id = if mapped_event_type == "assay.adapter.ucp.discovery.requested" {
            Some(discovery_id.unwrap_or_else(|| {
                notes.push("missing discovery.id -> substituted unknown-discovery".to_string());
                unmapped_fields_count += 1;
                "unknown-discovery".to_string()
            }))
        } else {
            discovery_id
        };

        let order_id = if mapped_event_type == "assay.adapter.ucp.order.requested" {
            Some(order_id.unwrap_or_else(|| {
                notes.push("missing order.id -> substituted unknown-order".to_string());
                unmapped_fields_count += 1;
                "unknown-order".to_string()
            }))
        } else {
            order_id
        };

        let checkout_id = if mapped_event_type == "assay.adapter.ucp.checkout.updated" {
            Some(checkout_id.unwrap_or_else(|| {
                notes.push("missing checkout.id -> substituted unknown-checkout".to_string());
                unmapped_fields_count += 1;
                "unknown-checkout".to_string()
            }))
        } else {
            checkout_id
        };

        let fulfillment_id = if mapped_event_type == "assay.adapter.ucp.fulfillment.updated" {
            Some(fulfillment_id.unwrap_or_else(|| {
                notes.push("missing fulfillment.id -> substituted unknown-fulfillment".to_string());
                unmapped_fields_count += 1;
                "unknown-fulfillment".to_string()
            }))
        } else {
            fulfillment_id
        };

        let message_id = if mapped_event_type == "assay.adapter.ucp.message" {
            Some(message_id.unwrap_or_else(|| {
                notes.push("missing message.id -> substituted unknown-message".to_string());
                unmapped_fields_count += 1;
                "unknown-message".to_string()
            }))
        } else {
            message_id
        };

        let adapter = self.adapter();
        let timestamp = timestamp_field(&packet, "timestamp").unwrap_or_else(default_time);
        let primary_id = primary_id_for_event(
            mapped_event_type,
            &actor_id,
            discovery_id.as_deref(),
            order_id.as_deref(),
            checkout_id.as_deref(),
            fulfillment_id.as_deref(),
            message_id.as_deref(),
        );
        let run_id = format!("ucp:{primary_id}");
        let payload = build_payload(
            adapter.adapter_id,
            adapter.adapter_version,
            &version,
            event_type.as_deref(),
            &packet,
            &actor_id,
            discovery_id.as_deref(),
            order_id.as_deref(),
            checkout_id.as_deref(),
            fulfillment_id.as_deref(),
            message_id.as_deref(),
            unmapped_fields_count,
        );

        let event = EvidenceEvent::new(
            mapped_event_type,
            "urn:assay:adapter:ucp",
            run_id,
            0,
            payload,
        )
        .with_subject(primary_id)
        .with_time(timestamp);

        let lossiness_level = if unmapped_fields_count == 0 {
            LossinessLevel::None
        } else if unmapped_fields_count <= 2 {
            LossinessLevel::Low
        } else {
            LossinessLevel::High
        };

        Ok(AdapterBatch {
            events: vec![event],
            lossiness: LossinessReport {
                lossiness_level,
                unmapped_fields_count,
                raw_payload_ref: Some(raw_ref),
                notes,
            },
        })
    }
}

fn parse_packet(payload: &[u8]) -> AdapterResult<Value> {
    serde_json::from_slice(payload).map_err(|err| {
        AdapterError::new(
            AdapterErrorKind::Measurement,
            format!("invalid UCP payload JSON: {err}"),
        )
    })
}

fn validate_protocol(packet: &Value) -> AdapterResult<()> {
    let protocol = string_field(packet, "protocol");
    if protocol.as_deref() != Some(PROTOCOL_NAME) {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "protocol must be 'ucp'",
        ));
    }

    Ok(())
}

fn observed_version(packet: &Value, protocol_version: Option<&str>) -> AdapterResult<String> {
    string_field(packet, "version")
        .or_else(|| protocol_version.map(ToOwned::to_owned))
        .ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::Measurement,
                "missing required field: version",
            )
        })
}

fn validate_supported_version(version: &str) -> AdapterResult<()> {
    if version != PROTOCOL_VERSION {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!("unsupported UCP version: {version}"),
        ));
    }

    Ok(())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToOwned::to_owned)
}

fn nested_string_field(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToOwned::to_owned)
}

fn timestamp_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    let raw = value.get(key)?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn default_time() -> DateTime<Utc> {
    Utc.timestamp_opt(DEFAULT_TIME_SECS, 0)
        .single()
        .expect("default timestamp must be valid")
}

fn map_event_type(event_type: Option<&str>) -> Option<&'static str> {
    match event_type {
        Some("discovery.requested") => Some("assay.adapter.ucp.discovery.requested"),
        Some("order.requested") => Some("assay.adapter.ucp.order.requested"),
        Some("checkout.updated") => Some("assay.adapter.ucp.checkout.updated"),
        Some("fulfillment.updated") => Some("assay.adapter.ucp.fulfillment.updated"),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn primary_id_for_event<'a>(
    mapped_event_type: &str,
    actor_id: &'a str,
    discovery_id: Option<&'a str>,
    order_id: Option<&'a str>,
    checkout_id: Option<&'a str>,
    fulfillment_id: Option<&'a str>,
    message_id: Option<&'a str>,
) -> &'a str {
    match mapped_event_type {
        "assay.adapter.ucp.discovery.requested" => discovery_id.unwrap_or(actor_id),
        "assay.adapter.ucp.order.requested" => order_id.unwrap_or(actor_id),
        "assay.adapter.ucp.checkout.updated" => checkout_id.unwrap_or(actor_id),
        "assay.adapter.ucp.fulfillment.updated" => fulfillment_id.unwrap_or(actor_id),
        _ => message_id.unwrap_or(actor_id),
    }
}

fn count_unmapped_top_level_fields(packet: &Value) -> u32 {
    let Some(obj) = packet.as_object() else {
        return 0;
    };

    obj.keys()
        .filter(|key| {
            !matches!(
                key.as_str(),
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
        })
        .count() as u32
}

#[allow(clippy::too_many_arguments)]
fn build_payload(
    adapter_id: &str,
    adapter_version: &str,
    version: &str,
    upstream_event_type: Option<&str>,
    packet: &Value,
    actor_id: &str,
    discovery_id: Option<&str>,
    order_id: Option<&str>,
    checkout_id: Option<&str>,
    fulfillment_id: Option<&str>,
    message_id: Option<&str>,
    unmapped_fields_count: u32,
) -> Value {
    let mut payload = Map::new();
    payload.insert(
        "adapter_id".to_string(),
        Value::String(adapter_id.to_string()),
    );
    payload.insert(
        "adapter_version".to_string(),
        Value::String(adapter_version.to_string()),
    );
    payload.insert(
        "protocol".to_string(),
        Value::String(PROTOCOL_NAME.to_string()),
    );
    payload.insert(
        "protocol_name".to_string(),
        Value::String(PROTOCOL_NAME.to_string()),
    );
    payload.insert(
        "protocol_version".to_string(),
        Value::String(version.to_string()),
    );
    if let Some(event_type) = upstream_event_type {
        payload.insert(
            "upstream_event_type".to_string(),
            Value::String(event_type.to_string()),
        );
    }
    if let Some(actor) = normalized_object_field(packet, "actor", Some(actor_id)) {
        payload.insert("actor".to_string(), actor);
    }
    if let Some(discovery) = normalized_object_field(packet, "discovery", discovery_id) {
        payload.insert("discovery".to_string(), discovery);
    }
    if let Some(order) = normalized_object_field(packet, "order", order_id) {
        payload.insert("order".to_string(), order);
    }
    if let Some(checkout) = normalized_object_field(packet, "checkout", checkout_id) {
        payload.insert("checkout".to_string(), checkout);
    }
    if let Some(fulfillment) = normalized_object_field(packet, "fulfillment", fulfillment_id) {
        payload.insert("fulfillment".to_string(), fulfillment);
    }
    if let Some(message) = normalized_object_field(packet, "message", message_id) {
        payload.insert("message".to_string(), message);
    }
    if let Some(attributes) = packet.get("attributes") {
        payload.insert("attributes".to_string(), normalize_json(attributes));
    }
    payload.insert(
        "unmapped_fields_count".to_string(),
        Value::Number(unmapped_fields_count.into()),
    );
    Value::Object(payload)
}

fn normalized_object_field(packet: &Value, key: &str, id: Option<&str>) -> Option<Value> {
    let mut object = match packet.get(key) {
        Some(Value::Object(map)) => match normalize_json(&Value::Object(map.clone())) {
            Value::Object(normalized) => normalized,
            _ => unreachable!("normalized object must remain object"),
        },
        Some(other) => {
            let mut fallback = Map::new();
            fallback.insert("value".to_string(), normalize_json(other));
            fallback
        }
        None => {
            id?;
            Map::new()
        }
    };

    if let Some(id) = id {
        object.insert("id".to_string(), Value::String(id.to_string()));
    }

    Some(Value::Object(object))
}

fn normalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let mut normalized = Map::new();
            for key in keys {
                normalized.insert(key.clone(), normalize_json(&map[key]));
            }
            Value::Object(normalized)
        }
        Value::Array(values) => Value::Array(values.iter().map(normalize_json).collect()),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_adapter_api::digest_canonical_json;
    use proptest::prelude::*;
    use sha2::Digest;
    use std::{fs, path::PathBuf};

    struct TestWriter;

    impl AttachmentWriter for TestWriter {
        fn write_raw_payload(
            &self,
            payload: &[u8],
            media_type: &str,
        ) -> AdapterResult<assay_adapter_api::RawPayloadRef> {
            Ok(assay_adapter_api::RawPayloadRef {
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
}
