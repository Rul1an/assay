use serde_json::Value;

use assay_adapter_api::{AdapterError, AdapterErrorKind, AdapterResult};

use super::fields::string_field;

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
