//! A2A adapter freeze skeleton.
//!
//! Step1 only freezes protocol metadata and the crate surface. Runtime mapping
//! is intentionally deferred to the Step2 implementation slice.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterError, AdapterErrorKind, AdapterInput, AdapterResult,
    AttachmentWriter, ConvertOptions, ProtocolAdapter, ProtocolDescriptor,
};

const PROTOCOL_NAME: &str = "a2a";
const SPEC_VERSION_RANGE: &str = ">=0.2 <1.0";
const SPEC_URL: &str = "https://google.github.io/A2A/";
const SCHEMA_ID: &str = "a2a.message.v0_2";

/// A2A adapter freeze skeleton.
#[derive(Debug, Default, Clone, Copy)]
pub struct A2aAdapter;

impl ProtocolAdapter for A2aAdapter {
    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: SPEC_VERSION_RANGE.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supported_event_types: vec![
                "assay.adapter.a2a.agent.capabilities".to_string(),
                "assay.adapter.a2a.task.requested".to_string(),
                "assay.adapter.a2a.task.updated".to_string(),
                "assay.adapter.a2a.artifact.shared".to_string(),
                "assay.adapter.a2a.message".to_string(),
            ],
            supported_spec_versions: vec![SPEC_VERSION_RANGE.to_string()],
            supports_strict: true,
            supports_lenient: true,
        }
    }

    fn convert(
        &self,
        _input: AdapterInput<'_>,
        _options: &ConvertOptions,
        _attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch> {
        Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "A2A Step1 freeze: runtime translation is not implemented yet",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_adapter_api::{AttachmentWriter, RawPayloadRef};

    struct StubWriter;

    impl AttachmentWriter for StubWriter {
        fn write_raw_payload(
            &self,
            payload: &[u8],
            media_type: &str,
        ) -> AdapterResult<RawPayloadRef> {
            Ok(RawPayloadRef {
                sha256: format!("sha256:{}", payload.len()),
                size_bytes: payload.len() as u64,
                media_type: media_type.to_string(),
            })
        }
    }

    #[test]
    fn exposes_frozen_a2a_metadata() {
        let adapter = A2aAdapter;
        let protocol = adapter.protocol();
        let capabilities = adapter.capabilities();

        assert_eq!(protocol.name, "a2a");
        assert_eq!(protocol.spec_version, ">=0.2 <1.0");
        assert_eq!(protocol.schema_id.as_deref(), Some("a2a.message.v0_2"));
        assert!(capabilities.supports_strict);
        assert!(capabilities.supports_lenient);
        assert!(capabilities
            .supported_event_types
            .contains(&"assay.adapter.a2a.task.requested".to_string()));
    }

    #[test]
    fn convert_is_explicitly_stubbed_in_step1() {
        let adapter = A2aAdapter;
        let writer = StubWriter;
        let err = adapter
            .convert(
                AdapterInput {
                    payload: br#"{"protocol":"a2a"}"#,
                    media_type: "application/json",
                    protocol_version: Some("0.2"),
                },
                &ConvertOptions::default(),
                &writer,
            )
            .expect_err("step1 skeleton must not silently partially convert");

        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert!(err.message.contains("not implemented yet"));
    }
}
