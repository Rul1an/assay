use serde_json::Value;

use assay_adapter_api::{AdapterError, AdapterErrorKind, AdapterResult};

use super::{fields::string_field, PROTOCOL_NAME};

pub(super) fn parse_packet(payload: &[u8]) -> AdapterResult<Value> {
    serde_json::from_slice(payload).map_err(|err| {
        AdapterError::new(
            AdapterErrorKind::Measurement,
            format!("invalid UCP payload JSON: {err}"),
        )
    })
}

pub(super) fn validate_protocol(packet: &Value) -> AdapterResult<()> {
    let protocol = string_field(packet, "protocol");
    if protocol.as_deref() != Some(PROTOCOL_NAME) {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "protocol must be 'ucp'",
        ));
    }

    Ok(())
}
