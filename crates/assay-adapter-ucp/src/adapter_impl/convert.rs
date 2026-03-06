use assay_adapter_api::{
    validate_json_shape, AdapterBatch, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertMode, ConvertOptions, LossinessLevel, LossinessReport,
};
use assay_evidence::types::EvidenceEvent;

use super::{
    fields::{default_time, nested_string_field, string_field, timestamp_field},
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
            notes.push("unsupported event_type -> emitted generic UCP message event".to_string());
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

    let adapter = super::adapter_descriptor();
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
