use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolDescriptor,
};

pub(super) const PROTOCOL_NAME: &str = "ucp";
pub(super) const PROTOCOL_VERSION: &str = "v2026-01-23";
pub(super) const SUPPORTED_RELEASE_LINE: &str = "v2026-01-23";
pub(super) const SCHEMA_ID: &str = "ucp.packet.v2026_01_23";
pub(super) const SPEC_URL: &str = "https://github.com/google-agentic-commerce/ucp";
pub(super) const DEFAULT_TIME_SECS: i64 = 1_700_300_000;
pub(super) const ADAPTER_ID: &str = "assay-adapter-ucp";

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
            "assay.adapter.ucp.discovery.requested".to_string(),
            "assay.adapter.ucp.order.requested".to_string(),
            "assay.adapter.ucp.checkout.updated".to_string(),
            "assay.adapter.ucp.fulfillment.updated".to_string(),
            "assay.adapter.ucp.message".to_string(),
        ],
        supported_spec_versions: vec![SUPPORTED_RELEASE_LINE.to_string()],
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

mod convert;
mod fields;
mod mapping;
mod parse;
mod payload;
mod version;

#[cfg(test)]
mod tests;
