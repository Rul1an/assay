use assay_adapter_api::{
    validate_json_shape, AdapterBatch, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertMode, ConvertOptions, LossinessLevel, LossinessReport,
};
use assay_evidence::types::EvidenceEvent;

use super::{
    fields::{
        default_time, nested_string_array_field, nested_string_field, string_field, timestamp_field,
    },
    mapping::{count_unmapped_top_level_fields, map_event_type, primary_id_for_event},
    parse::{parse_packet, validate_protocol},
    payload::build_payload,
    version::{observed_version, validate_supported_version},
};

pub(super) fn convert(
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
            notes.push("unsupported event_type -> emitted generic A2A message event".to_string());
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

    let adapter = super::adapter_descriptor();
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
        adapter.adapter_id,
        adapter.adapter_version,
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
