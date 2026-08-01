//! Local, typed limit vocabulary for the bounded MCP-shaped OTLP/JSON decoder.
//!
//! The mechanism/vocabulary split follows `assay_common::limits`: the shared
//! [`assay_common::limits::LimitReader`] is reused only for the one dimension with stream
//! semantics (source bytes). Every other ceiling is a property of the decode traversal, not of a
//! byte stream, so it is enforced inside the visitor and named here in OTLP terms. This
//! vocabulary does not travel outside the module.
//!
//! Every limit is inclusive: an input at exactly the limit is accepted, an input one past it is
//! rejected. That boundary is load-bearing and each dimension has a test proving both sides.

/// Ceilings applied before any untrusted content is retained.
///
/// Dimensions are independent on purpose: a document can be small in source bytes while hostile
/// in nesting depth, and declared metadata (`droppedAttributesCount` and friends) never
/// substitutes for the observed counts these ceilings govern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OtlpIngestLimits {
    /// Bytes taken from the source stream, before JSON decoding.
    pub(crate) max_source_bytes: u64,
    /// Cumulative decoded scalar bytes across the whole document. Charge model: a JSON string
    /// (member name or value) charges its UTF-8 byte length; a number charges 8; a boolean or
    /// null charges 1. The model is part of the contract so the boundary is testable exactly.
    pub(crate) max_decoded_bytes: u64,
    /// Deepest permitted container nesting. Every JSON object or array entered counts one level,
    /// including the root and skipped unknown subtrees.
    pub(crate) max_nesting_depth: u64,
    /// Spans across all `resourceSpans[].scopeSpans[].spans[]` lists combined.
    pub(crate) max_span_count: u64,
    /// Entries in any single recognized attribute list (resource or span attributes).
    pub(crate) max_attribute_count: u64,
    /// UTF-8 bytes of a single attribute `key`.
    pub(crate) max_attribute_key_bytes: u64,
    /// Cumulative decoded scalar bytes inside a single attribute `value` subtree, using the same
    /// charge model as `max_decoded_bytes`.
    pub(crate) max_attribute_value_bytes: u64,
}

impl OtlpIngestLimits {
    /// Defaults sized for the pinned `otel-mcp-ingest-v0` corpus: every benign fixture fits with
    /// headroom, and each locked hostile fixture crosses exactly the ceiling its locked purpose
    /// names (depth 19 > 16; oversized attribute value 802 > 512).
    pub(crate) fn corpus_v0() -> Self {
        Self {
            max_source_bytes: 64 * 1024,
            max_decoded_bytes: 64 * 1024,
            max_nesting_depth: 16,
            max_span_count: 128,
            max_attribute_count: 64,
            max_attribute_key_bytes: 256,
            max_attribute_value_bytes: 512,
        }
    }
}

/// Which independent ceiling a rejected input crossed.
///
/// Deliberately not `#[non_exhaustive]`: adding a dimension must break every consumer match at
/// compile time rather than fall through a wildcard onto the wrong classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OtlpLimitDimension {
    SourceBytes,
    DecodedBytes,
    NestingDepth,
    SpanCount,
    AttributeCount,
    AttributeKeyBytes,
    AttributeValueBytes,
}

/// Where in the OTLP/JSON structure a shape or duplicate fault was observed. The site names are
/// ours, never input-derived, so an error stays actionable without echoing hostile bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeSite {
    Root,
    ResourceSpans,
    ResourceSpansEntry,
    Resource,
    ScopeSpans,
    ScopeSpansEntry,
    Spans,
    Span,
    AttributeList,
    AttributeEntry,
    AttributeValue,
    Status,
    /// An unknown container being skipped without retention. Duplicate members fail closed
    /// there too: skipping is not a license to accept a document a traversed path would refuse.
    SkippedContainer,
}

/// A structurally required span field, named from the pinned OTLP schema rather than from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpanField {
    TraceId,
    SpanId,
    Kind,
    StatusCode,
}

/// A recognized MCP observation attribute, named from the pinned semconv rather than from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecognizedAttribute {
    MethodName,
    OperationName,
    ToolName,
    RequestId,
    ErrorType,
}

/// Typed, value-free rejection. No variant carries attacker-controlled bytes: limits carry the
/// configured ceiling, structural faults carry sites and field names from our own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum OtlpIngestError {
    #[error("exceeded {dimension:?} limit of {limit}")]
    LimitExceeded {
        dimension: OtlpLimitDimension,
        limit: u64,
    },
    /// The source ended before the document did (short read / truncation).
    #[error("source truncated before the document ended")]
    TruncatedSource,
    /// The bytes are not a well-formed JSON document. Value-free by construction: the syntax
    /// error's position and token text are dropped at the boundary.
    #[error("malformed JSON document")]
    MalformedJson,
    /// A recognized container had the wrong JSON shape (for example a non-object where the
    /// pinned schema requires an object).
    #[error("unexpected shape at {0:?}")]
    UnexpectedShape(ShapeSite),
    /// A traversed JSON object stated the same member twice.
    #[error("duplicate member at {0:?}")]
    DuplicateField(ShapeSite),
    /// One attribute list stated the same `key` twice.
    #[error("duplicate attribute key")]
    DuplicateAttributeKey,
    /// An `AnyValue` object stated more than one value member.
    #[error("conflicting attribute value members")]
    ConflictingAttributeValue,
    #[error("missing required span field {0:?}")]
    MissingRequiredSpanField(SpanField),
    #[error("malformed span field {0:?}")]
    MalformedSpanField(SpanField),
    /// A recognized MCP attribute carried a value type the pinned semconv does not define for it.
    #[error("recognized attribute {0:?} has unsupported value type")]
    RecognizedAttributeWrongType(RecognizedAttribute),
    /// The source reader failed for a reason other than a configured ceiling.
    #[error("source read failed")]
    Io,
}
