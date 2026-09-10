//! A2A adapter for translating selected A2A evidence projection profile packets
//! into canonical Assay evidence events.
//!
//! # Profile Boundary
//! This adapter implements an internal Assay projection profile (`assay.adapter.a2a.legacy-projection.v0`),
//! translating synthetic and observed 0.x task/agent/artifact lifecycle events into canonical
//! evidence. It is a selected evidence projection, not an implementation of the full A2A wire
//! specification (such as A2A 1.0 protobuf/gRPC wire objects).

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolAdapter, ProtocolDescriptor,
};

mod adapter_impl;

pub use adapter_impl::{PROFILE_NAME, SUPPORTED_SPEC_VERSIONS};

/// A2A adapter for the legacy evidence projection profile.
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
