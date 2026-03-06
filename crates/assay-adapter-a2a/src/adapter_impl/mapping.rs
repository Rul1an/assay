use serde_json::Value;

pub(super) fn map_event_type(event_type: Option<&str>) -> Option<&'static str> {
    match event_type {
        Some("agent.capabilities") => Some("assay.adapter.a2a.agent.capabilities"),
        Some("task.requested") => Some("assay.adapter.a2a.task.requested"),
        Some("task.updated") => Some("assay.adapter.a2a.task.updated"),
        Some("artifact.shared") => Some("assay.adapter.a2a.artifact.shared"),
        _ => None,
    }
}

pub(super) fn primary_id_for_event<'a>(
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

pub(super) fn count_unmapped_top_level_fields(packet: &Value) -> u32 {
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
