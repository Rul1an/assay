use serde_json::{Map, Value};

use super::PROTOCOL_NAME;

#[allow(clippy::too_many_arguments)]
pub(super) fn build_payload(
    adapter_id: &str,
    adapter_version: &str,
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
