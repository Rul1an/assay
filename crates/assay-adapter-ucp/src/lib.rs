//! UCP adapter Step1 skeleton for freezing the protocol contract before runtime mapping lands.

use assay_adapter_api::{
    AdapterBatch, AdapterCapabilities, AdapterDescriptor, AdapterError, AdapterErrorKind,
    AdapterInput, AdapterResult, AttachmentWriter, ConvertOptions, ProtocolAdapter,
    ProtocolDescriptor,
};

const PROTOCOL_NAME: &str = "ucp";
const PROTOCOL_VERSION: &str = "v2026-01-23";
const SUPPORTED_RELEASE_LINE: &str = "v2026-01-23";
const SCHEMA_ID: &str = "ucp.packet.v2026_01_23";
const SPEC_URL: &str = "https://github.com/google-agentic-commerce/ucp";
const ADAPTER_ID: &str = "assay-adapter-ucp";

/// UCP adapter Step1 skeleton.
#[derive(Debug, Default, Clone, Copy)]
pub struct UcpAdapter;

impl ProtocolAdapter for UcpAdapter {
    fn adapter(&self) -> AdapterDescriptor {
        AdapterDescriptor {
            adapter_id: ADAPTER_ID,
            adapter_version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn protocol(&self) -> ProtocolDescriptor {
        ProtocolDescriptor {
            name: PROTOCOL_NAME.to_string(),
            spec_version: PROTOCOL_VERSION.to_string(),
            schema_id: Some(SCHEMA_ID.to_string()),
            spec_url: Some(SPEC_URL.to_string()),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
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

    fn convert(
        &self,
        _input: AdapterInput<'_>,
        _options: &ConvertOptions,
        _attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch> {
        Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "runtime translation is not implemented yet for UCP Step1",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_adapter_api::{AttachmentWriter, ConvertMode, RawPayloadRef};

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
    fn protocol_metadata_uses_frozen_release_tag() {
        let adapter = UcpAdapter;
        assert_eq!(adapter.adapter().adapter_id, "assay-adapter-ucp");
        assert!(!adapter.adapter().adapter_version.is_empty());
        assert_eq!(adapter.protocol().name, "ucp");
        assert_eq!(adapter.protocol().spec_version, "v2026-01-23");
        assert_eq!(
            adapter.capabilities().supported_spec_versions,
            vec!["v2026-01-23"]
        );
    }

    #[test]
    fn convert_explicitly_stubs_runtime_translation() {
        let adapter = UcpAdapter;
        let writer = StubWriter;
        let input = AdapterInput {
            payload: br#"{}"#,
            media_type: "application/json",
            protocol_version: Some("v2026-01-23"),
        };

        let err = adapter
            .convert(
                input,
                &ConvertOptions {
                    mode: ConvertMode::Strict,
                    ..ConvertOptions::default()
                },
                &writer,
            )
            .expect_err("step1 skeleton must not claim runtime support");

        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert!(err
            .message
            .contains("runtime translation is not implemented yet"));
    }
}
