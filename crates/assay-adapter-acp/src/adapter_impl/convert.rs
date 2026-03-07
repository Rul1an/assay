use super::{
    lossiness::classify_lossiness,
    mapping::{count_unmapped_top_level_fields, map_event_type},
    normalize::normalize_json,
    raw_payload::write_raw_payload_ref,
};
use assay_adapter_api::{
    validate_json_shape, AdapterBatch, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertMode, ConvertOptions, LossinessReport,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Map, Value};

use crate::{ADAPTER_ID, DEFAULT_TIME_SECS, PROTOCOL_NAME, SPEC_VERSION};

pub(crate) fn convert_impl(
    input: AdapterInput<'_>,
    options: &ConvertOptions,
    attachments: &dyn AttachmentWriter,
) -> AdapterResult<AdapterBatch> {
    let raw_ref = write_raw_payload_ref(input.payload, input.media_type, options, attachments)?;
    let packet = parse_packet(input.payload)?;
    validate_json_shape(&packet, options.max_json_depth, options.max_array_length)?;
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
            notes.push("unsupported event_type -> emitted generic ACP packet event".to_string());
            unmapped_fields_count += 1;
            "assay.adapter.acp.packet"
        }
    };

    let timestamp = timestamp_field(&packet, "timestamp").unwrap_or_else(default_time);
    let run_id = format!("acp:{}", packet_id);
    let payload = build_payload(
        ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
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

    Ok(AdapterBatch {
        events: vec![event],
        lossiness: LossinessReport {
            lossiness_level: classify_lossiness(unmapped_fields_count),
            unmapped_fields_count,
            raw_payload_ref: Some(raw_ref),
            notes,
        },
    })
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

#[allow(clippy::too_many_arguments)]
fn build_payload(
    adapter_id: &str,
    adapter_version: &str,
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
