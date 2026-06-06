use assay_adapter_api::{
    AdapterCapabilities, LossinessLevel, LossinessReport, ProtocolDescriptor, RawPayloadRef,
};

pub(super) const TRUSTED_ADAPTER_ID: &str = "assay-adapter-acp";
pub(super) const UNTRUSTED_ADAPTER_ID: &str = "assay-adapter-acp-fork";

pub(super) fn clean_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        supported_event_types: vec!["tool.decision".to_string()],
        supported_spec_versions: vec![">=1.0 <2.0".to_string()],
        supports_strict: true,
        supports_lenient: true,
    }
}

pub(super) fn clean_protocol() -> ProtocolDescriptor {
    ProtocolDescriptor {
        name: "acp".to_string(),
        spec_version: "1.0".to_string(),
        schema_id: Some("acp.packet".to_string()),
        spec_url: None,
    }
}

pub(super) fn clean_lossiness() -> LossinessReport {
    LossinessReport {
        lossiness_level: LossinessLevel::None,
        unmapped_fields_count: 0,
        raw_payload_ref: Some(RawPayloadRef {
            sha256: "sha256:abc123def456".to_string(),
            size_bytes: 1024,
            media_type: "application/json".to_string(),
        }),
        notes: vec![],
    }
}
