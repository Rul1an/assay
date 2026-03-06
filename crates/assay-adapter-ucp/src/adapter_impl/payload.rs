use serde_json::{Map, Value};

use super::PROTOCOL_NAME;

#[allow(clippy::too_many_arguments)]
pub(super) fn build_payload(
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

pub(super) fn normalized_object_field(
    packet: &Value,
    key: &str,
    id: Option<&str>,
) -> Option<Value> {
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

pub(super) fn normalize_json(value: &Value) -> Value {
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
