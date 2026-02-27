//! Stable contracts for protocol adapters that translate external protocol payloads
//! into canonical Assay evidence events.

use assay_evidence::types::EvidenceEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result type for adapter operations.
pub type AdapterResult<T> = Result<T, AdapterError>;

/// Stable protocol metadata exposed by each adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolDescriptor {
    /// Short protocol identifier such as `acp` or `a2a`.
    pub name: String,
    /// Supported specification version for the adapter implementation.
    pub spec_version: String,
    /// Optional schema identifier for payload validation.
    pub schema_id: Option<String>,
    /// Optional human-facing specification URL.
    pub spec_url: Option<String>,
}

/// Stable adapter implementation metadata exposed by each adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterDescriptor {
    /// Stable adapter crate or implementation identifier.
    pub adapter_id: &'static str,
    /// Adapter build/version string.
    pub adapter_version: &'static str,
}

/// Capabilities exposed by the adapter for review and routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdapterCapabilities {
    /// Event types this adapter may emit.
    pub supported_event_types: Vec<String>,
    /// Supported upstream protocol versions or ranges.
    pub supported_spec_versions: Vec<String>,
    /// Whether strict conversion mode is implemented.
    pub supports_strict: bool,
    /// Whether lenient conversion mode is implemented.
    pub supports_lenient: bool,
}

/// Conversion strictness for protocol translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConvertMode {
    /// Fail on malformed or unmappable critical protocol data.
    #[default]
    Strict,
    /// Emit evidence plus explicit lossiness metadata and raw payload reference.
    Lenient,
}

/// Conversion options shared by all adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConvertOptions {
    /// Strictness mode for conversion.
    pub mode: ConvertMode,
    /// Optional payload size ceiling enforced before deep parsing.
    pub max_payload_bytes: Option<u64>,
}

/// Raw protocol input supplied to an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterInput<'a> {
    /// Raw protocol payload bytes.
    pub payload: &'a [u8],
    /// Media type for the source payload.
    pub media_type: &'a str,
    /// Optional explicit protocol version observed at ingest time.
    pub protocol_version: Option<&'a str>,
}

/// Digest-backed reference to a preserved raw payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawPayloadRef {
    /// SHA-256 digest of the preserved payload.
    pub sha256: String,
    /// Size in bytes of the preserved payload.
    pub size_bytes: u64,
    /// Media type of the preserved payload.
    pub media_type: String,
}

/// Lossiness classification for a conversion result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LossinessLevel {
    /// No known loss.
    #[default]
    None,
    /// Minor field-level loss.
    Low,
    /// Material translation loss.
    High,
}

/// Explicit accounting for translation loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LossinessReport {
    /// Overall lossiness level.
    pub lossiness_level: LossinessLevel,
    /// Number of unmapped fields encountered during translation.
    pub unmapped_fields_count: u32,
    /// Preserved raw payload reference, when available.
    pub raw_payload_ref: Option<RawPayloadRef>,
    /// Optional human-facing notes for diagnostics.
    pub notes: Vec<String>,
}

/// Batch conversion result emitted by an adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AdapterBatch {
    /// Canonical evidence events emitted by the adapter.
    pub events: Vec<EvidenceEvent>,
    /// Explicit lossiness metadata for the batch.
    pub lossiness: LossinessReport,
}

/// Error category for adapter failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterErrorKind {
    /// Invalid adapter configuration.
    Config,
    /// Measurement or contract failure while parsing/validating input.
    Measurement,
    /// Host-side storage or attachment backend failure.
    Infrastructure,
    /// Upstream protocol version unsupported by this adapter.
    UnsupportedProtocolVersion,
    /// Strict mode rejected a lossy conversion.
    StrictLossinessViolation,
}

/// Stable adapter error surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{kind:?}: {message}")]
pub struct AdapterError {
    /// Error classification.
    pub kind: AdapterErrorKind,
    /// Human-readable failure message.
    pub message: String,
}

impl AdapterError {
    /// Create a new typed adapter error.
    #[must_use]
    pub fn new(kind: AdapterErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Host-provided interface for preserving raw protocol payloads.
pub trait AttachmentWriter {
    /// Persist a raw payload and return its digest-backed reference.
    fn write_raw_payload(&self, payload: &[u8], media_type: &str) -> AdapterResult<RawPayloadRef>;
}

/// Stable contract implemented by protocol-specific adapters.
pub trait ProtocolAdapter {
    /// Return stable adapter implementation metadata.
    fn adapter(&self) -> AdapterDescriptor;

    /// Return stable protocol metadata.
    fn protocol(&self) -> ProtocolDescriptor;

    /// Return supported adapter capabilities.
    fn capabilities(&self) -> AdapterCapabilities;

    /// Convert a raw protocol payload into canonical evidence events.
    fn convert(
        &self,
        input: AdapterInput<'_>,
        options: &ConvertOptions,
        attachments: &dyn AttachmentWriter,
    ) -> AdapterResult<AdapterBatch>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::types::EvidenceEvent;

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

    struct StubAdapter;

    impl ProtocolAdapter for StubAdapter {
        fn adapter(&self) -> AdapterDescriptor {
            AdapterDescriptor {
                adapter_id: "assay-adapter-acp",
                adapter_version: env!("CARGO_PKG_VERSION"),
            }
        }

        fn protocol(&self) -> ProtocolDescriptor {
            ProtocolDescriptor {
                name: "acp".to_string(),
                spec_version: "2.11.0".to_string(),
                schema_id: Some("acp.packet".to_string()),
                spec_url: Some("https://example.invalid/acp".to_string()),
            }
        }

        fn capabilities(&self) -> AdapterCapabilities {
            AdapterCapabilities {
                supported_event_types: vec!["assay.adapter.acp.packet".to_string()],
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
            if matches!(options.mode, ConvertMode::Strict) && input.payload.is_empty() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Measurement,
                    "empty payload in strict mode",
                ));
            }

            let raw_ref = attachments.write_raw_payload(input.payload, input.media_type)?;
            let event = EvidenceEvent::new(
                "assay.adapter.acp.packet",
                "urn:assay:adapter:acp",
                "run-1",
                0,
                serde_json::json!({"media_type": input.media_type}),
            );

            Ok(AdapterBatch {
                events: vec![event],
                lossiness: LossinessReport {
                    lossiness_level: LossinessLevel::None,
                    unmapped_fields_count: 0,
                    raw_payload_ref: Some(raw_ref),
                    notes: Vec::new(),
                },
            })
        }
    }

    #[test]
    fn strict_empty_payload_fails() {
        let adapter = StubAdapter;
        let writer = StubWriter;
        let input = AdapterInput {
            payload: &[],
            media_type: "application/json",
            protocol_version: Some("2.11.0"),
        };
        let err = adapter
            .convert(input, &ConvertOptions::default(), &writer)
            .expect_err("strict empty payload should fail");
        assert_eq!(err.kind, AdapterErrorKind::Measurement);
    }

    #[test]
    fn lenient_path_emits_event_and_raw_ref() {
        let adapter = StubAdapter;
        let writer = StubWriter;
        let input = AdapterInput {
            payload: br#"{"kind":"checkout"}"#,
            media_type: "application/json",
            protocol_version: Some("2.11.0"),
        };
        let batch = adapter
            .convert(
                input,
                &ConvertOptions {
                    mode: ConvertMode::Lenient,
                    max_payload_bytes: Some(4096),
                },
                &writer,
            )
            .expect("lenient conversion should succeed");
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.lossiness.lossiness_level, LossinessLevel::None);
        assert_eq!(
            batch.lossiness.raw_payload_ref.expect("raw ref").size_bytes,
            19
        );
    }

    #[test]
    fn adapter_descriptor_exposes_identity() {
        let adapter = StubAdapter;
        let descriptor = adapter.adapter();

        assert_eq!(descriptor.adapter_id, "assay-adapter-acp");
        assert!(!descriptor.adapter_version.is_empty());
    }
}
