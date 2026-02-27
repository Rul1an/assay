//! A2A adapter MVP for translating selected A2A packets into canonical Assay evidence events.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertMode, ConvertOptions, LossinessLevel, LossinessReport,
    ProtocolAdapter, ProtocolDescriptor,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Map, Value};

const PROTOCOL_NAME: &str = "a2a";
const SPEC_VERSION_RANGE: &str = ">=0.2 <1.0";
const SCHEMA_ID: &str = "a2a.message.v0_2";
const SPEC_URL: &str = "https://google.github.io/A2A/";
const DEFAULT_TIME_SECS: i64 = 1_700_100_000;

/// A2A adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct A2aAdapter;

impl ProtocolAdapter for A2aAdapter {
    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: SPEC_VERSION_RANGE.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supported_event_types: vec![
                "assay.adapter.a2a.agent.capabilities".to_string(),
                "assay.adapter.a2a.task.requested".to_string(),
                "assay.adapter.a2a.task.updated".to_string(),
                "assay.adapter.a2a.artifact.shared".to_string(),
                "assay.adapter.a2a.message".to_string(),
            ],
            supported_spec_versions: vec![SPEC_VERSION_RANGE.to_string()],
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
        validate_protocol(&packet)?;
        let version = observed_version(&packet, input.protocol_version)?;
        validate_supported_version(&version)?;

        let mut notes = Vec::new();
        let mut unmapped_fields_count = count_unmapped_top_level_fields(&packet);

        let event_type = string_field(&packet, "event_type");
        let agent_id = nested_string_field(&packet, &["agent", "id"]);
        let agent_name = nested_string_field(&packet, &["agent", "name"]);
        let agent_role = nested_string_field(&packet, &["agent", "role"]);
        let agent_capabilities = nested_string_array_field(&packet, &["agent", "capabilities"]);
        let task_id = nested_string_field(&packet, &["task", "id"]);
        let task_status = nested_string_field(&packet, &["task", "status"]);
        let task_kind = nested_string_field(&packet, &["task", "kind"]);
        let artifact_id = nested_string_field(&packet, &["artifact", "id"]);
        let artifact_name = nested_string_field(&packet, &["artifact", "name"]);
        let artifact_media_type = nested_string_field(&packet, &["artifact", "media_type"]);
        let message_id = nested_string_field(&packet, &["message", "id"]);
        let message_role = nested_string_field(&packet, &["message", "role"]);
        let mapped_event_type = map_event_type(event_type.as_deref());

        if matches!(options.mode, ConvertMode::Strict) {
            if agent_id.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: agent.id",
                ));
            }
            if mapped_event_type.is_none() {
                return Err(AdapterError::new(
                    AdapterErrorKind::StrictLossinessViolation,
                    "unsupported event_type in strict mode",
                ));
            }
            if matches!(
                mapped_event_type,
                Some("assay.adapter.a2a.task.requested" | "assay.adapter.a2a.task.updated")
            ) && task_id.is_none()
            {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: task.id",
                ));
            }
            if matches!(mapped_event_type, Some("assay.adapter.a2a.artifact.shared"))
                && artifact_id.is_none()
            {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "missing required field: artifact.id",
                ));
            }
        }

        let agent_id = agent_id.unwrap_or_else(|| {
            notes.push("missing agent.id -> substituted unknown-agent".to_string());
            unmapped_fields_count += 1;
            "unknown-agent".to_string()
        });

        let mapped_event_type = match mapped_event_type {
            Some(name) => name,
            None => {
                notes.push(
                    "unsupported event_type -> emitted generic A2A message event".to_string(),
                );
                unmapped_fields_count += 1;
                "assay.adapter.a2a.message"
            }
        };

        let task_id = if mapped_event_type.starts_with("assay.adapter.a2a.task.") {
            Some(task_id.unwrap_or_else(|| {
                notes.push("missing task.id -> substituted unknown-task".to_string());
                unmapped_fields_count += 1;
                "unknown-task".to_string()
            }))
        } else {
            task_id
        };

        let artifact_id = if mapped_event_type == "assay.adapter.a2a.artifact.shared" {
            Some(artifact_id.unwrap_or_else(|| {
                notes.push("missing artifact.id -> substituted unknown-artifact".to_string());
                unmapped_fields_count += 1;
                "unknown-artifact".to_string()
            }))
        } else {
            artifact_id
        };

        let message_id = message_id.or_else(|| {
            if mapped_event_type == "assay.adapter.a2a.message" {
                notes.push("missing message.id -> substituted unknown-message".to_string());
                unmapped_fields_count += 1;
                Some("unknown-message".to_string())
            } else {
                None
            }
        });

        let timestamp = timestamp_field(&packet, "timestamp").unwrap_or_else(default_time);
        let primary_id = primary_id_for_event(
            mapped_event_type,
            &agent_id,
            task_id.as_deref(),
            artifact_id.as_deref(),
            message_id.as_deref(),
        );
        let run_id = format!("a2a:{primary_id}");
        let payload = build_payload(
            &version,
            event_type.as_deref(),
            &agent_id,
            agent_name.as_deref(),
            agent_role.as_deref(),
            agent_capabilities.as_ref(),
            task_id.as_deref(),
            task_status.as_deref(),
            task_kind.as_deref(),
            artifact_id.as_deref(),
            artifact_name.as_deref(),
            artifact_media_type.as_deref(),
            message_id.as_deref(),
            message_role.as_deref(),
            packet.get("attributes"),
            unmapped_fields_count,
        );

        let event = EvidenceEvent::new(
            mapped_event_type,
            "urn:assay:adapter:a2a",
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
            format!("invalid A2A payload JSON: {err}"),
        )
    })
}

fn validate_protocol(packet: &Value) -> AdapterResult<()> {
    let protocol = string_field(packet, "protocol");
    if protocol.as_deref() != Some(PROTOCOL_NAME) {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "protocol must be 'a2a'",
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
    let Some((major, minor)) = parse_version(version) else {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!("unsupported A2A version: {version}"),
        ));
    };

    if major != 0 || minor < 2 {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!("unsupported A2A version: {version}"),
        ));
    }

    Ok(())
}

fn parse_version(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
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

fn nested_string_array_field(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }

    let arr = current.as_array()?;
    let mut values: Vec<String> = arr
        .iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect();
    values.sort();
    Some(values)
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
        Some("agent.capabilities") => Some("assay.adapter.a2a.agent.capabilities"),
        Some("task.requested") => Some("assay.adapter.a2a.task.requested"),
        Some("task.updated") => Some("assay.adapter.a2a.task.updated"),
        Some("artifact.shared") => Some("assay.adapter.a2a.artifact.shared"),
        _ => None,
    }
}

fn primary_id_for_event<'a>(
    mapped_event_type: &str,
    agent_id: &'a str,
    task_id: Option<&'a str>,
    artifact_id: Option<&'a str>,
    message_id: Option<&'a str>,
) -> &'a str {
    match mapped_event_type {
        "assay.adapter.a2a.task.requested" | "assay.adapter.a2a.task.updated" => {
            task_id.unwrap_or(agent_id)
        }
        "assay.adapter.a2a.artifact.shared" => artifact_id.unwrap_or(agent_id),
        "assay.adapter.a2a.message" => message_id.unwrap_or(agent_id),
        _ => agent_id,
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
                    | "agent"
                    | "task"
                    | "artifact"
                    | "message"
                    | "attributes"
            )
        })
        .count() as u32
}

#[allow(clippy::too_many_arguments)]
fn build_payload(
    version: &str,
    upstream_event_type: Option<&str>,
    agent_id: &str,
    agent_name: Option<&str>,
    agent_role: Option<&str>,
    agent_capabilities: Option<&Vec<String>>,
    task_id: Option<&str>,
    task_status: Option<&str>,
    task_kind: Option<&str>,
    artifact_id: Option<&str>,
    artifact_name: Option<&str>,
    artifact_media_type: Option<&str>,
    message_id: Option<&str>,
    message_role: Option<&str>,
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
        Value::String(version.to_string()),
    );
    if let Some(event_type) = upstream_event_type {
        payload.insert(
            "upstream_event_type".to_string(),
            Value::String(event_type.to_string()),
        );
    }

    let mut agent = Map::new();
    agent.insert("id".to_string(), Value::String(agent_id.to_string()));
    if let Some(name) = agent_name {
        agent.insert("name".to_string(), Value::String(name.to_string()));
    }
    if let Some(role) = agent_role {
        agent.insert("role".to_string(), Value::String(role.to_string()));
    }
    if let Some(capabilities) = agent_capabilities {
        agent.insert(
            "capabilities".to_string(),
            Value::Array(
                capabilities
                    .iter()
                    .map(|cap| Value::String(cap.to_string()))
                    .collect(),
            ),
        );
    }
    payload.insert("agent".to_string(), Value::Object(agent));

    if let Some(task_id) = task_id {
        let mut task = Map::new();
        task.insert("id".to_string(), Value::String(task_id.to_string()));
        if let Some(status) = task_status {
            task.insert("status".to_string(), Value::String(status.to_string()));
        }
        if let Some(kind) = task_kind {
            task.insert("kind".to_string(), Value::String(kind.to_string()));
        }
        payload.insert("task".to_string(), Value::Object(task));
    }

    if let Some(artifact_id) = artifact_id {
        let mut artifact = Map::new();
        artifact.insert("id".to_string(), Value::String(artifact_id.to_string()));
        if let Some(name) = artifact_name {
            artifact.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(media_type) = artifact_media_type {
            artifact.insert(
                "media_type".to_string(),
                Value::String(media_type.to_string()),
            );
        }
        payload.insert("artifact".to_string(), Value::Object(artifact));
    }

    if let Some(message_id) = message_id {
        let mut message = Map::new();
        message.insert("id".to_string(), Value::String(message_id.to_string()));
        if let Some(role) = message_role {
            message.insert("role".to_string(), Value::String(role.to_string()));
        }
        payload.insert("message".to_string(), Value::Object(message));
    }

    if let Some(attributes) = attributes {
        payload.insert("attributes".to_string(), normalize_json(attributes));
    }

    payload.insert(
        "unmapped_fields_count".to_string(),
        Value::Number(unmapped_fields_count.into()),
    );

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
        Value::Array(values) => {
            if values.iter().all(|item| item.as_str().is_some()) {
                let mut strings: Vec<_> = values
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect();
                strings.sort();
                Value::Array(strings.into_iter().map(Value::String).collect())
            } else {
                Value::Array(values.iter().map(normalize_json).collect())
            }
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_adapter_api::RawPayloadRef;
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf};

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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ci/fixtures/adr026/a2a/v0.2")
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
        assert_eq!(digest_json(&first), digest_json(&second));
        assert_eq!(
            first.events[0].payload["agent"]["capabilities"],
            serde_json::json!(["agent.describe", "artifacts.share", "tasks.update"])
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
                },
                &writer,
            )
            .expect("lenient missing task id should substitute unknown task");

        assert_eq!(batch.events[0].type_, "assay.adapter.a2a.task.requested");
        assert_eq!(batch.events[0].subject.as_deref(), Some("unknown-task"));
        assert!(batch.lossiness.unmapped_fields_count >= 1);
        assert!(batch.lossiness.raw_payload_ref.is_some());
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
                },
                &writer,
            )
            .expect_err("oversized payload must fail measurement contract");
        assert_eq!(err.kind, AdapterErrorKind::Measurement);
    }
}
