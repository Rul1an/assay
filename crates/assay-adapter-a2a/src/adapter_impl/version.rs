use serde_json::Value;

use assay_adapter_api::{AdapterError, AdapterErrorKind, AdapterResult};

use super::{fields::string_field, PROFILE_NAME};

pub const SUPPORTED_SPEC_VERSIONS: &[&str] = &["0.2", "0.2.0", "0.3.1"];

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
    if !SUPPORTED_SPEC_VERSIONS.contains(&version) {
        return Err(AdapterError::new(
            AdapterErrorKind::UnsupportedProtocolVersion,
            format!(
                "unsupported A2A version: {version}; supported versions for profile '{PROFILE_NAME}' are: {}",
                SUPPORTED_SPEC_VERSIONS.join(", ")
            ),
        ));
    }

    Ok(())
}
