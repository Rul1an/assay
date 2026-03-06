use serde_json::Value;

use assay_adapter_api::{AdapterError, AdapterErrorKind, AdapterResult};

use super::{fields::string_field, PROTOCOL_VERSION};

pub(super) fn observed_version(
    packet: &Value,
    protocol_version: Option<&str>,
) -> AdapterResult<String> {
    string_field(packet, "version")
        .or_else(|| protocol_version.map(ToOwned::to_owned))
        .ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::Measurement,
                "missing required field: version",
            )
        })
}

pub(super) fn validate_supported_version(version: &str) -> AdapterResult<()> {
    if version != PROTOCOL_VERSION {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!("unsupported UCP version: {version}"),
        ));
    }

    Ok(())
}
