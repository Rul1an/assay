use serde_json::Value;

pub(super) fn map_event_type(event_type: Option<&str>) -> Option<&'static str> {
    match event_type {
        Some("discovery.requested") => Some("assay.adapter.ucp.discovery.requested"),
        Some("order.requested") => Some("assay.adapter.ucp.order.requested"),
        Some("checkout.updated") => Some("assay.adapter.ucp.checkout.updated"),
        Some("fulfillment.updated") => Some("assay.adapter.ucp.fulfillment.updated"),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn primary_id_for_event<'a>(
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
