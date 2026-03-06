//! UCP adapter MVP for translating a governance-relevant subset of UCP packets into canonical
//! Assay evidence events.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolAdapter, ProtocolDescriptor,
};

mod adapter_impl;

/// UCP adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct UcpAdapter;

impl ProtocolAdapter for UcpAdapter {
    fn adapter(&self) -> AdapterDescriptor {
        adapter_impl::adapter_descriptor()
    }

    fn protocol(&self) -> ProtocolDescriptor {
        adapter_impl::protocol_descriptor()
    }

    fn capabilities(&self) -> AdapterCapabilities {
        adapter_impl::capabilities()
    }

    fn convert(
        &self,
        input: AdapterInput<'_>,
        options: &ConvertOptions,
        attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch> {
        adapter_impl::convert(input, options, attachments)
    }
}
