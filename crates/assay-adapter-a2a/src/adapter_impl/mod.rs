use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolDescriptor,
};

pub const PROFILE_NAME: &str = "assay.adapter.a2a.legacy-projection.v0";
pub(super) const PROTOCOL_NAME: &str = "a2a";
pub(super) const PROTOCOL_VERSION: &str = "0.2.0";
pub(super) const SCHEMA_ID: &str = PROFILE_NAME;
pub(super) const SPEC_URL: &str = "https://google.github.io/A2A/";
pub(super) const DEFAULT_TIME_SECS: i64 = 1_700_100_000;
pub(super) const ADAPTER_ID: &str = "assay-adapter-a2a";

mod convert;
mod discovery;
mod fields;
mod handoff;
mod mapping;
mod parse;
mod payload;
mod version;

pub use version::SUPPORTED_SPEC_VERSIONS;

pub(super) fn adapter_descriptor() -> AdapterDescriptor {
    AdapterDescriptor {
        adapter_id: ADAPTER_ID,
        adapter_version: env!("CARGO_PKG_VERSION"),
    }
}

pub(super) fn protocol_descriptor() -> ProtocolDescriptor {
    ProtocolDescriptor {
        name: PROTOCOL_NAME.to_string(),
        spec_version: PROTOCOL_VERSION.to_string(),
        schema_id: Some(SCHEMA_ID.to_string()),
        spec_url: Some(SPEC_URL.to_string()),
    }
}

pub(super) fn capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        supported_event_types: vec![
            "assay.adapter.a2a.agent.capabilities".to_string(),
            "assay.adapter.a2a.task.requested".to_string(),
            "assay.adapter.a2a.task.updated".to_string(),
            "assay.adapter.a2a.artifact.shared".to_string(),
            "assay.adapter.a2a.message".to_string(),
        ],
        supported_spec_versions: SUPPORTED_SPEC_VERSIONS
            .iter()
            .map(|&s| s.to_string())
            .collect(),
        supports_strict: true,
        supports_lenient: true,
    }
}

pub(super) fn convert(
    input: AdapterInput<'_>,
    options: &ConvertOptions,
    attachments: &dyn AttachmentWriter,
) -> AdapterResult<AdapterBatch> {
    convert::convert(input, options, attachments)
}

#[cfg(test)]
mod tests;
