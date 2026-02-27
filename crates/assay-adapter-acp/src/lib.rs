//! ACP adapter MVP for translating selected ACP packets into canonical Assay evidence events.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertMode, ConvertOptions, LossinessLevel, LossinessReport,
    ProtocolAdapter, ProtocolDescriptor,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Map, Value};

const PROTOCOL_NAME: &str = "acp";
const SPEC_VERSION: &str = "2.11.0";
const SCHEMA_ID: &str = "acp.packet.v2_11_0";
const SPEC_URL: &str = "https://example.invalid/specs/acp/2.11.0";
const DEFAULT_TIME_SECS: i64 = 1_700_000_000;

/// ACP adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcpAdapter;

impl ProtocolAdapter for AcpAdapter {
    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: SPEC_VERSION.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supported_event_types: vec![
                "assay.adapter.acp.intent.created".to_string(),
                "assay.adapter.acp.checkout.requested".to_string(),
                "assay.adapter.acp.packet".to_string(),
            ],
            supported_spec_versions: vec![">=2.11 <3.0".to_string()],
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
                    format!("payload exceeds max_payload_bytes ({})", limit),
                ));
            }
        }

        let raw_ref = attachments.write_raw_payload(input.payload, input.media_type)?;
        let packet = parse_packet(input.payload)?;
        validate_protocol_version(&packet)?;

        let mut notes = Vec::new();
        let mut unmapped_fields_count = count_unmapped_top_level_fields(&packet);

        let packet_id = string_field(&packet, "packet_id");
        let actor_id = nested_string_field(&packet, &["actor", "id"]);
        let actor_role = nested_string_field(&packet, &["actor", "role"]);
        let event_type = string_field(&packet, "event_type");
        let intent_id = nested_string_field(&packet, &["intent", "id"]);
        let intent_kind = nested_string_field(&packet, &["intent", "kind"]);
        let event_name = map_event_type(event_type.as_deref());

        if matches!(options.mode, ConvertMode::Strict) {
            if packet_id.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: packet_id",
                ));
            }
            if actor_id.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: actor.id",
                ));
            }
            if event_name.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::StrictLossinessViolation,
                    "unsupported event_type in strict mode",
                ));
            }
        }

        let packet_id = packet_id.unwrap_or_else(|| {
            notes.push("missing packet_id -> substituted unknown-packet".to_string());
            unmapped_fields_count += 1;
            "unknown-packet".to_string()
        });
        let actor_id = actor_id.unwrap_or_else(|| {
            notes.push("missing actor.id -> substituted unknown-actor".to_string());
            unmapped_fields_count += 1;
            "unknown-actor".to_string()
        });

        let mapped_event_type = match event_name {
            Some(name) => name,
            None => {
                notes
                    .push("unsupported event_type -> emitted generic ACP packet event".to_string());
                unmapped_fields_count += 1;
                "assay.adapter.acp.packet"
            }
        };

        let timestamp = timestamp_field(&packet, "timestamp").unwrap_or_else(default_time);
        let run_id = format!("acp:{}", packet_id);
        let payload = build_payload(
            &packet_id,
            &actor_id,
            actor_role.as_deref(),
            event_type.as_deref(),
            intent_id.as_deref(),
            intent_kind.as_deref(),
            packet.get("attributes"),
            unmapped_fields_count,
        );

        let event = EvidenceEvent::new(
            mapped_event_type,
            "urn:assay:adapter:acp",
            run_id,
            0,
            payload,
        )
        .with_subject(packet_id)
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
            format!("invalid ACP payload JSON: {err}"),
        )
    })
}

fn validate_protocol_version(packet: &Value) -> AdapterResult<()> {
    let protocol = string_field(packet, "protocol");
    if protocol.as_deref() != Some(PROTOCOL_NAME) {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "protocol must be 'acp'",
        ));
    }

    let version = string_field(packet, "version");
    if version.as_deref() != Some(SPEC_VERSION) {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!("unsupported ACP version: {}", version.unwrap_or_default()),
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
        Some("intent.created") => Some("assay.adapter.acp.intent.created"),
        Some("checkout.requested") => Some("assay.adapter.acp.checkout.requested"),
        _ => None,
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
                    | "packet_id"
                    | "event_type"
                    | "timestamp"
                    | "actor"
                    | "intent"
                    | "attributes"
            )
        })
        .count() as u32
}

#[allow(clippy::too_many_arguments)]
fn build_payload(
    packet_id: &str,
    actor_id: &str,
    actor_role: Option<&str>,
    upstream_event_type: Option<&str>,
    intent_id: Option<&str>,
    intent_kind: Option<&str>,
    attributes: Option<&Value>,
    unmapped_fields_count: u32,
) -> Value {
    let mut payload = Map::new();
    payload.insert(
        "protocol".to_string(),
        Value::String(PROTOCOL_NAME.to_string()),
    );
    payload.insert(
        "protocol_version".to_string(),
        Value::String(SPEC_VERSION.to_string()),
    );
    payload.insert(
        "packet_id".to_string(),
        Value::String(packet_id.to_string()),
    );
    payload.insert("actor_id".to_string(), Value::String(actor_id.to_string()));
    payload.insert(
        "unmapped_fields_count".to_string(),
        Value::Number(unmapped_fields_count.into()),
    );
    if let Some(role) = actor_role {
        payload.insert("actor_role".to_string(), Value::String(role.to_string()));
    }
    if let Some(event_type) = upstream_event_type {
        payload.insert(
            "upstream_event_type".to_string(),
            Value::String(event_type.to_string()),
        );
    }
    if let Some(id) = intent_id {
        payload.insert("intent_id".to_string(), Value::String(id.to_string()));
    }
    if let Some(kind) = intent_kind {
        payload.insert("intent_kind".to_string(), Value::String(kind.to_string()));
    }
    if let Some(attributes) = attributes {
        payload.insert("attributes".to_string(), normalize_json(attributes));
    }
    Value::Object(payload)
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
    use assay_adapter_api::{AttachmentWriter, ConvertMode, ConvertOptions, RawPayloadRef};
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::PathBuf;

    struct TestWriter;

    impl AttachmentWriter for TestWriter {
        fn write_raw_payload(
            &self,
            payload: &[u8],
            media_type: &str,
        ) -> AdapterResult<RawPayloadRef> {
            let mut hasher = Sha256::new();
            hasher.update(payload);
            Ok(RawPayloadRef {
                sha256: hex::encode(hasher.finalize()),
                size_bytes: payload.len() as u64,
                media_type: media_type.to_string(),
            })
        }
    }

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/ci/fixtures/adr026/acp/v2.11.0")
    }

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(fixture_dir().join(name)).expect("fixture must exist")
    }

    fn digest_json<T: Serialize>(value: &T) -> String {
        let encoded = serde_json::to_vec(value).expect("serializes");
        let mut hasher = Sha256::new();
        hasher.update(encoded);
        hex::encode(hasher.finalize())
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
        assert_eq!(digest_json(&first), digest_json(&second));
        assert_eq!(first.events[0].type_, "assay.adapter.acp.intent.created");
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
            digest_json(&first.events[0].payload),
            digest_json(&second.events[0].payload)
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
                },
                &writer,
            )
            .expect_err("oversized payload must fail measurement contract");
        assert_eq!(err.kind, AdapterErrorKind::Measurement);
    }
}
