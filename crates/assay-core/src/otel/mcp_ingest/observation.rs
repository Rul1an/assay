//! Typed observations extracted from MCP-shaped OTLP/JSON spans.
//!
//! Only the explicit MCP observation fields named in issue #1931 are represented. Upstream
//! provenance is a static pinned mapping — [`UpstreamField`] names the exact upstream field
//! each observation kind is extracted from, rather than being carried per observation — and the
//! whole decode result carries the semconv pin, so a consumer always knows which
//! Development-status convention revision named these fields.
//!
//! `mcp.protocol.version` is producer-self-reported telemetry, kept deliberately separate from
//! the transport-level `EraResolution` contract: [`SpanProtocolVersion`] has no conflict state,
//! because a span cannot manufacture the MCP-defined header/body conflict.

/// The pinned semconv revision that defines the recognized attribute names. Matches the
/// `semconv` entry in `tests/fixtures/otel-mcp-ingest-v0/upstream.lock.json`.
pub(crate) const SEMCONV_PIN: &str =
    "open-telemetry/semantic-conventions-genai@434c91dcc34ed038e3048c07720ddfed2c6bddfc";

/// Upstream provenance of one extracted observation: which pinned field it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamField {
    /// OTLP span `traceId`.
    TraceId,
    /// OTLP span `spanId`.
    SpanId,
    /// OTLP span `kind`.
    SpanKind,
    /// OTLP span `status.code`.
    StatusCode,
    /// Semconv attribute `mcp.method.name`.
    McpMethodName,
    /// Semconv attribute `gen_ai.operation.name`.
    GenAiOperationName,
    /// Semconv attribute `gen_ai.tool.name`.
    GenAiToolName,
    /// Fixture-pinned attribute `jsonrpc.request.id`.
    JsonRpcRequestId,
    /// Semconv attribute `mcp.protocol.version`.
    McpProtocolVersion,
    /// Stable general attribute `error.type` (used by the Development MCP document).
    ErrorType,
    /// Semconv attribute `rpc.response.status_code`.
    RpcResponseStatusCode,
}

impl UpstreamField {
    /// The exact upstream field name, for receipts that must state where a value came from.
    pub(crate) fn upstream_name(self) -> &'static str {
        match self {
            UpstreamField::TraceId => "traceId",
            UpstreamField::SpanId => "spanId",
            UpstreamField::SpanKind => "kind",
            UpstreamField::StatusCode => "status.code",
            UpstreamField::McpMethodName => "mcp.method.name",
            UpstreamField::GenAiOperationName => "gen_ai.operation.name",
            UpstreamField::GenAiToolName => "gen_ai.tool.name",
            UpstreamField::JsonRpcRequestId => "jsonrpc.request.id",
            UpstreamField::McpProtocolVersion => "mcp.protocol.version",
            UpstreamField::ErrorType => "error.type",
            UpstreamField::RpcResponseStatusCode => "rpc.response.status_code",
        }
    }
}

/// OTLP span kind, from the pinned `trace.proto` enum. Proto3 enums are open, so a future numeric
/// value is retained only as the value-free `Unknown` state rather than mislabeled malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpanKind {
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
    Unknown,
}

/// OTLP span status code, from the pinned `trace.proto` enum. Proto3 enums are open; values outside
/// the pinned vocabulary become `Unknown`. Absent status is the proto default and ordinary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusObservation {
    Absent,
    Unset,
    Ok,
    Error,
    Unknown,
}

/// What `mcp.method.name` said. Only the explicit tool-call method is recognized; any other
/// method is recorded value-free, because unrecognized method strings are attacker-chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum MethodObservation {
    #[default]
    Absent,
    ToolsCall,
    OtherMethod,
}

/// What `gen_ai.operation.name` said, with the same value-free rule for unrecognized names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum OperationObservation {
    #[default]
    Absent,
    ExecuteTool,
    OtherOperation,
}

/// What `jsonrpc.request.id` said, preserving the upstream value type instead of coercing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestIdObservation {
    String(String),
}

/// What `error.type` said. Present values are bounded by the attribute-value ceiling before
/// retention.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ErrorTypeObservation {
    #[default]
    Absent,
    Present(String),
}

/// What `rpc.response.status_code` said. The pinned semconv declares a string value; absence is
/// ordinary when no JSON-RPC error code was reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum RpcResponseStatusObservation {
    #[default]
    Absent,
    Present(String),
}

/// The span-reported `mcp.protocol.version`, as its own provenance-bearing observation.
///
/// This is **not** an [`crate::mcp::era::EraResolution`] and never feeds one: the span value is
/// producer-self-reported telemetry, not a transport envelope and not request metadata, so it
/// cannot be a third source into the two-source era contract and there is deliberately no
/// conflict state here. Absent is an ordinary observation (the attribute is Recommended, not
/// Required). Malformed and unsupported are typed states, not decode failures. Only the calendar
/// date syntax and the supported-version vocabulary are shared with the era module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum SpanProtocolVersion {
    #[default]
    Absent,
    /// Present but not a real calendar date, or not a string at all. Value-free: an unreadable
    /// version token is attacker-chosen and retaining it adds nothing actionable.
    Malformed,
    PresentSupported(String),
    PresentUnsupported(String),
}

/// Bounded identity reported by the enclosing OTLP instrumentation scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InstrumentationScopeObservation {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

/// One decoded MCP-shaped span, carrying only the explicit observation fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpSpanObservation {
    /// 32-char hex trace id, validated case-insensitively and normalized to lowercase.
    pub(crate) trace_id: String,
    /// 16-char hex span id, validated case-insensitively and normalized to lowercase.
    pub(crate) span_id: String,
    /// Optional 16-char parent span id, normalized to lowercase when present.
    pub(crate) parent_span_id: Option<String>,
    pub(crate) instrumentation_scope: Option<InstrumentationScopeObservation>,
    pub(crate) kind: SpanKind,
    pub(crate) method: MethodObservation,
    pub(crate) operation: OperationObservation,
    /// `gen_ai.tool.name`, bounded by the attribute-value ceiling before retention.
    pub(crate) tool_name: Option<String>,
    pub(crate) request_id: Option<RequestIdObservation>,
    pub(crate) protocol_version: SpanProtocolVersion,
    pub(crate) status: StatusObservation,
    pub(crate) error_type: ErrorTypeObservation,
    pub(crate) rpc_response_status: RpcResponseStatusObservation,
}

/// The decode result for one OTLP/JSON document: extracted spans plus the semconv pin they were
/// extracted under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpResourceSpansObservation {
    pub(crate) semconv_pin: &'static str,
    pub(crate) spans: Vec<McpSpanObservation>,
}
