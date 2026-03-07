use serde_json::Value;

pub(crate) fn map_event_type(event_type: Option<&str>) -> Option<&'static str> {
    match event_type {
        Some("intent.created") => Some("assay.adapter.acp.intent.created"),
        Some("checkout.requested") => Some("assay.adapter.acp.checkout.requested"),
        _ => None,
    }
}

pub(crate) fn count_unmapped_top_level_fields(packet: &Value) -> u32 {
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
