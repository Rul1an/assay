//! ACP adapter MVP for translating selected ACP packets into canonical Assay evidence events.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolAdapter, ProtocolDescriptor,
};

mod adapter_impl;

#[cfg(test)]
mod tests;

const PROTOCOL_NAME: &str = "acp";
const SPEC_VERSION: &str = "2.11.0";
const SCHEMA_ID: &str = "acp.packet.v2_11_0";
const SPEC_URL: &str = "https://example.invalid/specs/acp/2.11.0";
const DEFAULT_TIME_SECS: i64 = 1_700_000_000;
const ADAPTER_ID: &str = "assay-adapter-acp";

/// ACP adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcpAdapter;

impl ProtocolAdapter for AcpAdapter {
    fn adapter(&self) -> AdapterDescriptor {
        AdapterDescriptor {
            adapter_id: ADAPTER_ID,
            adapter_version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: SPEC_VERSION.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supported_event_types: vec![
                "assay.adapter.acp.intent.created".to_string(),
                "assay.adapter.acp.checkout.requested".to_string(),
                "assay.adapter.acp.packet".to_string(),
            ],
            supported_spec_versions: vec![">=2.11 <3.0".to_string()],
            supports_strict: true,
            supports_lenient: true,
        }
    }

    fn convert(
        &self,
        input: AdapterInput<'_>,
        options: &ConvertOptions,
        attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch> {
        adapter_impl::convert_impl(input, options, attachments)
    }
}
