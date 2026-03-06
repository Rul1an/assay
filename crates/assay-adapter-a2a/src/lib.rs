//! A2A adapter MVP for translating selected A2A packets into canonical Assay evidence events.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolAdapter, ProtocolDescriptor,
};

mod adapter_impl;

/// A2A adapter MVP.
#[derive(Debug, Default, Clone, Copy)]
pub struct A2aAdapter;

impl ProtocolAdapter for A2aAdapter {
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
