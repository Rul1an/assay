//! Behavioral tests for the Slice B bounded decoder.
//!
//! Every limit dimension is proven on both sides of its boundary (exact limit accepted,
//! limit+1 rejected), every hostile fixture locked in `upstream.lock.json` is rejected for its
//! locked purpose, and every rejection path is swept for attacker-content echo.

use std::io::Read;

use super::decode::decode_mcp_resource_spans;
use super::limits::{
    OtlpIngestError, OtlpIngestLimits, OtlpLimitDimension, RecognizedAttribute, ShapeSite,
    SpanField,
};
use super::observation::{
    ErrorTypeObservation, McpResourceSpansObservation, MethodObservation, OperationObservation,
    RequestIdObservation, SpanKind, SpanProtocolVersion, StatusObservation, UpstreamField,
    SEMCONV_PIN,
};

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/otel-mcp-ingest-v0"
);

const TRACE_ID: &str = "0af7651916cd43dd8448eb211c80319c";
const SPAN_ID: &str = "b7ad6b7169203331";

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURE_DIR}/{name}")).expect("fixture must exist")
}

fn decode_str(
    doc: &str,
    limits: &OtlpIngestLimits,
) -> Result<McpResourceSpansObservation, OtlpIngestError> {
    decode_mcp_resource_spans(doc.as_bytes(), limits)
}

fn corpus_limits() -> OtlpIngestLimits {
    OtlpIngestLimits::corpus_v0()
}

/// A document with one resource entry, empty resource attributes, and the given span bodies.
fn doc_with_spans(spans: &[String]) -> String {
    format!(
        r#"{{"resourceSpans":[{{"resource":{{"attributes":[]}},"scopeSpans":[{{"spans":[{}]}}]}}]}}"#,
        spans.join(",")
    )
}

/// A minimal valid span carrying the given attribute entries.
fn span_with_attrs(attrs: &[String]) -> String {
    format!(
        r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","kind":3,"attributes":[{}]}}"#,
        attrs.join(",")
    )
}

fn attr_str(key: &str, value: &str) -> String {
    format!(r#"{{"key":"{key}","value":{{"stringValue":"{value}"}}}}"#)
}

fn doc_with_attrs(attrs: &[String]) -> String {
    doc_with_spans(&[span_with_attrs(attrs)])
}

fn assert_limit(
    result: Result<McpResourceSpansObservation, OtlpIngestError>,
    dimension: OtlpLimitDimension,
    limit: u64,
) {
    assert_eq!(
        result.expect_err("input past the ceiling must be rejected"),
        OtlpIngestError::LimitExceeded { dimension, limit }
    );
}

// --- Corpus acceptance and extraction -------------------------------------------------------

#[test]
fn client_fixture_decodes_and_extracts_explicit_fields() {
    let bytes = fixture_bytes("mcp_client_tools_call.json");
    let obs = decode_mcp_resource_spans(&bytes[..], &corpus_limits()).expect("benign corpus");
    assert_eq!(obs.semconv_pin, SEMCONV_PIN);
    assert_eq!(obs.spans.len(), 1);
    let span = &obs.spans[0];
    assert_eq!(span.trace_id, "b18fab63dd8f1d4d78179e2f7b5f7114");
    assert_eq!(span.span_id, "8a0bb4282ecf34fc");
    assert_eq!(span.kind, SpanKind::Client);
    assert_eq!(span.method, MethodObservation::ToolsCall);
    assert_eq!(span.operation, OperationObservation::ExecuteTool);
    assert_eq!(span.tool_name.as_deref(), Some("read_file"));
    assert_eq!(
        span.request_id,
        Some(RequestIdObservation::String("1".into()))
    );
    assert_eq!(
        span.protocol_version,
        SpanProtocolVersion::PresentSupported("2024-11-05".into())
    );
    assert_eq!(span.status, StatusObservation::Ok);
    assert_eq!(span.error_type, ErrorTypeObservation::Absent);
}

#[test]
fn server_fixture_decodes_as_server_kind() {
    let bytes = fixture_bytes("mcp_server_tools_call.json");
    let obs = decode_mcp_resource_spans(&bytes[..], &corpus_limits()).expect("benign corpus");
    assert_eq!(obs.spans.len(), 1);
    assert_eq!(obs.spans[0].kind, SpanKind::Server);
    assert_eq!(obs.spans[0].method, MethodObservation::ToolsCall);
}

#[test]
fn provenance_mapping_is_pinned() {
    assert_eq!(
        SEMCONV_PIN,
        "open-telemetry/semantic-conventions-genai@434c91dcc34ed038e3048c07720ddfed2c6bddfc"
    );
    let expected = [
        (UpstreamField::TraceId, "traceId"),
        (UpstreamField::SpanId, "spanId"),
        (UpstreamField::SpanKind, "kind"),
        (UpstreamField::StatusCode, "status.code"),
        (UpstreamField::McpMethodName, "mcp.method.name"),
        (UpstreamField::GenAiOperationName, "gen_ai.operation.name"),
        (UpstreamField::GenAiToolName, "gen_ai.tool.name"),
        (UpstreamField::JsonRpcRequestId, "jsonrpc.request.id"),
        (UpstreamField::McpProtocolVersion, "mcp.protocol.version"),
        (UpstreamField::ErrorType, "error.type"),
    ];
    for (field, name) in expected {
        assert_eq!(field.upstream_name(), name);
    }
}

// --- Hostile fixtures reject for their locked purpose ---------------------------------------

#[test]
fn hostile_deep_nesting_rejects_on_depth() {
    let bytes = fixture_bytes("hostile_deep_nesting.json");
    assert_limit(
        decode_mcp_resource_spans(&bytes[..], &corpus_limits()),
        OtlpLimitDimension::NestingDepth,
        corpus_limits().max_nesting_depth,
    );
}

#[test]
fn hostile_oversized_attribute_rejects_on_value_bytes() {
    let bytes = fixture_bytes("hostile_oversized_attribute.json");
    assert_limit(
        decode_mcp_resource_spans(&bytes[..], &corpus_limits()),
        OtlpLimitDimension::AttributeValueBytes,
        corpus_limits().max_attribute_value_bytes,
    );
}

#[test]
fn hostile_missing_required_fields_rejects_on_missing_trace_id() {
    let bytes = fixture_bytes("hostile_missing_required_fields.json");
    assert_eq!(
        decode_mcp_resource_spans(&bytes[..], &corpus_limits())
            .expect_err("span without ids must reject"),
        OtlpIngestError::MissingRequiredSpanField(SpanField::TraceId)
    );
}

// --- Exact limit accepted, limit + 1 rejected, per dimension --------------------------------

#[test]
fn source_bytes_exact_accepted_one_more_rejected() {
    let doc = doc_with_spans(&[]);
    let mut limits = corpus_limits();
    limits.max_source_bytes = doc.len() as u64;
    decode_str(&doc, &limits).expect("input of exactly the source ceiling fits");
    let padded = format!("{doc} ");
    assert_limit(
        decode_str(&padded, &limits),
        OtlpLimitDimension::SourceBytes,
        limits.max_source_bytes,
    );
}

#[test]
fn decoded_bytes_exact_accepted_one_more_rejected() {
    // Charge model: member-name strings charge their UTF-8 length; the empty string value
    // charges 0. `{"resourceSpans":[],"a":""}` charges 13 + 1 = 14 decoded bytes.
    let mut limits = corpus_limits();
    limits.max_decoded_bytes = 14;
    decode_str(r#"{"resourceSpans":[],"a":""}"#, &limits).expect("exactly at decoded ceiling");
    assert_limit(
        decode_str(r#"{"resourceSpans":[],"ab":""}"#, &limits),
        OtlpLimitDimension::DecodedBytes,
        14,
    );
}

#[test]
fn nesting_depth_exact_accepted_one_more_rejected() {
    // The span object sits at depth 7; an unknown member holding k nested arrays reaches 7 + k.
    let mut limits = corpus_limits();
    limits.max_nesting_depth = 9;
    let span_at_9 = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","x":[[]]}}"#);
    decode_str(&doc_with_spans(&[span_at_9]), &limits).expect("exactly at depth ceiling");
    let span_at_10 = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","x":[[[]]]}}"#);
    assert_limit(
        decode_str(&doc_with_spans(&[span_at_10]), &limits),
        OtlpLimitDimension::NestingDepth,
        9,
    );
}

#[test]
fn span_count_exact_accepted_one_more_rejected() {
    let mut limits = corpus_limits();
    limits.max_span_count = 2;
    let two = vec![span_with_attrs(&[]), span_with_attrs(&[])];
    decode_str(&doc_with_spans(&two), &limits).expect("exactly at span ceiling");
    let three = vec![
        span_with_attrs(&[]),
        span_with_attrs(&[]),
        span_with_attrs(&[]),
    ];
    assert_limit(
        decode_str(&doc_with_spans(&three), &limits),
        OtlpLimitDimension::SpanCount,
        2,
    );
}

#[test]
fn attribute_count_exact_accepted_one_more_rejected() {
    let mut limits = corpus_limits();
    limits.max_attribute_count = 3;
    let three: Vec<String> = (0..3).map(|i| attr_str(&format!("k{i}"), "v")).collect();
    decode_str(&doc_with_attrs(&three), &limits).expect("exactly at attribute ceiling");
    let four: Vec<String> = (0..4).map(|i| attr_str(&format!("k{i}"), "v")).collect();
    assert_limit(
        decode_str(&doc_with_attrs(&four), &limits),
        OtlpLimitDimension::AttributeCount,
        3,
    );
}

#[test]
fn resource_attribute_list_is_governed_by_the_same_ceiling() {
    let mut limits = corpus_limits();
    limits.max_attribute_count = 1;
    let doc = format!(
        r#"{{"resourceSpans":[{{"resource":{{"attributes":[{},{}]}},"scopeSpans":[]}}]}}"#,
        attr_str("a", "v"),
        attr_str("b", "v"),
    );
    assert_limit(
        decode_str(&doc, &limits),
        OtlpLimitDimension::AttributeCount,
        1,
    );
}

#[test]
fn attribute_key_bytes_exact_accepted_one_more_rejected() {
    let mut limits = corpus_limits();
    limits.max_attribute_key_bytes = 8;
    decode_str(&doc_with_attrs(&[attr_str("kkkkkkkk", "v")]), &limits)
        .expect("exactly at key ceiling");
    assert_limit(
        decode_str(&doc_with_attrs(&[attr_str("kkkkkkkkk", "v")]), &limits),
        OtlpLimitDimension::AttributeKeyBytes,
        8,
    );
}

#[test]
fn attribute_value_bytes_exact_accepted_one_more_rejected() {
    let mut limits = corpus_limits();
    limits.max_attribute_value_bytes = 8;
    decode_str(&doc_with_attrs(&[attr_str("k", "vvvvvvvv")]), &limits)
        .expect("exactly at value ceiling");
    assert_limit(
        decode_str(&doc_with_attrs(&[attr_str("k", "vvvvvvvvv")]), &limits),
        OtlpLimitDimension::AttributeValueBytes,
        8,
    );
}

#[test]
fn attribute_value_bytes_aggregate_across_nested_value() {
    // Two 4-byte strings inside one arrayValue aggregate to 8; a fifth byte crosses it.
    let mut limits = corpus_limits();
    limits.max_attribute_value_bytes = 8;
    let ok = r#"{"key":"k","value":{"arrayValue":{"values":[{"stringValue":"aaaa"},{"stringValue":"bbbb"}]}}}"#;
    decode_str(&doc_with_attrs(&[ok.to_string()]), &limits).expect("aggregate exactly at ceiling");
    let over = r#"{"key":"k","value":{"arrayValue":{"values":[{"stringValue":"aaaa"},{"stringValue":"bbbbb"}]}}}"#;
    assert_limit(
        decode_str(&doc_with_attrs(&[over.to_string()]), &limits),
        OtlpLimitDimension::AttributeValueBytes,
        8,
    );
}

// --- Short reads and source truncation ------------------------------------------------------

/// Yields one byte per `read` call: every read is short.
struct OneByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Read for OneByteReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.data[self.pos];
        self.pos += 1;
        Ok(1)
    }
}

/// Fails with a non-limit I/O error after a prefix.
struct FailingReader<'a> {
    prefix: &'a [u8],
    pos: usize,
}

impl Read for FailingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.prefix.len() {
            return Err(std::io::Error::other("backing store failed"));
        }
        let n = buf.len().min(self.prefix.len() - self.pos);
        buf[..n].copy_from_slice(&self.prefix[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[test]
fn one_byte_short_reads_decode_identically() {
    let bytes = fixture_bytes("mcp_client_tools_call.json");
    let whole = decode_mcp_resource_spans(&bytes[..], &corpus_limits()).expect("whole read");
    let chunked = decode_mcp_resource_spans(
        OneByteReader {
            data: &bytes,
            pos: 0,
        },
        &corpus_limits(),
    )
    .expect("short reads");
    assert_eq!(whole, chunked);
}

#[test]
fn one_byte_short_reads_reject_identically_at_the_source_boundary() {
    let doc = doc_with_spans(&[]);
    let padded = format!("{doc} ");
    let mut limits = corpus_limits();
    limits.max_source_bytes = doc.len() as u64;
    assert_limit(
        decode_mcp_resource_spans(
            OneByteReader {
                data: padded.as_bytes(),
                pos: 0,
            },
            &limits,
        ),
        OtlpLimitDimension::SourceBytes,
        limits.max_source_bytes,
    );
}

#[test]
fn truncated_source_is_typed_truncation() {
    let bytes = fixture_bytes("mcp_client_tools_call.json");
    assert_eq!(
        decode_mcp_resource_spans(&bytes[..100], &corpus_limits())
            .expect_err("truncated document must reject"),
        OtlpIngestError::TruncatedSource
    );
}

#[test]
fn empty_source_is_typed_truncation() {
    assert_eq!(
        decode_str("", &corpus_limits()).expect_err("empty input"),
        OtlpIngestError::TruncatedSource
    );
}

#[test]
fn non_limit_io_failure_is_typed_io_not_truncation() {
    let doc = doc_with_spans(&[]);
    assert_eq!(
        decode_mcp_resource_spans(
            FailingReader {
                prefix: &doc.as_bytes()[..10],
                pos: 0,
            },
            &corpus_limits(),
        )
        .expect_err("failing reader"),
        OtlpIngestError::Io
    );
}

// --- Malformed JSON -------------------------------------------------------------------------

#[test]
fn non_json_input_is_typed_malformed() {
    assert_eq!(
        decode_str("not json at all", &corpus_limits()).expect_err("garbage"),
        OtlpIngestError::MalformedJson
    );
}

#[test]
fn trailing_garbage_after_document_is_typed_malformed() {
    let doc = format!("{} x", doc_with_spans(&[]));
    assert_eq!(
        decode_str(&doc, &corpus_limits()).expect_err("trailing garbage"),
        OtlpIngestError::MalformedJson
    );
}

// --- Misleading metadata never governs ------------------------------------------------------

#[test]
fn declared_dropped_count_cannot_bypass_the_observed_attribute_ceiling() {
    let mut limits = corpus_limits();
    limits.max_attribute_count = 1;
    let span = format!(
        r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","droppedAttributesCount":0,"attributes":[{},{}]}}"#,
        attr_str("a", "v"),
        attr_str("b", "v"),
    );
    assert_limit(
        decode_str(&doc_with_spans(&[span]), &limits),
        OtlpLimitDimension::AttributeCount,
        1,
    );
}

#[test]
fn declared_dropped_count_cannot_reject_content_within_ceilings() {
    let span = format!(
        r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","droppedAttributesCount":4294967295,"attributes":[{}]}}"#,
        attr_str("a", "v"),
    );
    decode_str(&doc_with_spans(&[span]), &corpus_limits())
        .expect("declared metadata is not evidence about observed content");
}

// --- Duplicate members and duplicate attributes ---------------------------------------------

#[test]
fn duplicate_span_member_rejects() {
    let span = format!(r#"{{"traceId":"{TRACE_ID}","traceId":"{TRACE_ID}","spanId":"{SPAN_ID}"}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[span]), &corpus_limits()).expect_err("duplicate member"),
        OtlpIngestError::DuplicateField(ShapeSite::Span)
    );
}

#[test]
fn duplicate_root_member_rejects() {
    assert_eq!(
        decode_str(
            r#"{"resourceSpans":[],"resourceSpans":[]}"#,
            &corpus_limits()
        )
        .expect_err("duplicate root member"),
        OtlpIngestError::DuplicateField(ShapeSite::Root)
    );
}

#[test]
fn duplicate_attribute_key_rejects() {
    let doc = doc_with_attrs(&[attr_str("same", "a"), attr_str("same", "b")]);
    assert_eq!(
        decode_str(&doc, &corpus_limits()).expect_err("duplicate attribute key"),
        OtlpIngestError::DuplicateAttributeKey
    );
}

#[test]
fn duplicate_recognized_mcp_attribute_rejects() {
    let doc = doc_with_attrs(&[
        attr_str("mcp.method.name", "tools/call"),
        attr_str("mcp.method.name", "tools/call"),
    ]);
    assert_eq!(
        decode_str(&doc, &corpus_limits()).expect_err("duplicate MCP attribute"),
        OtlpIngestError::DuplicateAttributeKey
    );
}

// --- Non-object containers and shape faults -------------------------------------------------

#[test]
fn shape_faults_are_typed_per_site() {
    let cases: &[(&str, ShapeSite)] = &[
        (r#"[]"#, ShapeSite::Root),
        (r#"{"resourceSpans":5}"#, ShapeSite::ResourceSpans),
        (r#"{"resourceSpans":[5]}"#, ShapeSite::ResourceSpansEntry),
        (
            r#"{"resourceSpans":[{"resource":7,"scopeSpans":[]}]}"#,
            ShapeSite::Resource,
        ),
        (
            r#"{"resourceSpans":[{"scopeSpans":"x"}]}"#,
            ShapeSite::ScopeSpans,
        ),
        (
            r#"{"resourceSpans":[{"scopeSpans":[null]}]}"#,
            ShapeSite::ScopeSpansEntry,
        ),
        (
            r#"{"resourceSpans":[{"scopeSpans":[{"spans":{}}]}]}"#,
            ShapeSite::Spans,
        ),
        (
            r#"{"resourceSpans":[{"scopeSpans":[{"spans":["x"]}]}]}"#,
            ShapeSite::Span,
        ),
    ];
    for (doc, site) in cases {
        assert_eq!(
            decode_str(doc, &corpus_limits()).expect_err("shape fault"),
            OtlpIngestError::UnexpectedShape(*site),
            "site {site:?}"
        );
    }
}

#[test]
fn span_level_shape_faults_are_typed_per_site() {
    let attr_list_not_array =
        format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","attributes":"x"}}"#);
    let attr_entry_not_object =
        format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","attributes":[9]}}"#);
    let value_not_object = format!(
        r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","attributes":[{{"key":"k","value":"x"}}]}}"#
    );
    let status_not_object =
        format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","status":[]}}"#);
    let cases: &[(&String, ShapeSite)] = &[
        (&attr_list_not_array, ShapeSite::AttributeList),
        (&attr_entry_not_object, ShapeSite::AttributeEntry),
        (&value_not_object, ShapeSite::AttributeValue),
        (&status_not_object, ShapeSite::Status),
    ];
    for (span, site) in cases {
        assert_eq!(
            decode_str(&doc_with_spans(&[(*span).clone()]), &corpus_limits())
                .expect_err("shape fault"),
            OtlpIngestError::UnexpectedShape(*site),
            "site {site:?}"
        );
    }
}

#[test]
fn any_value_with_conflicting_members_rejects() {
    let two_variants = r#"{"key":"k","value":{"stringValue":"a","intValue":"1"}}"#.to_string();
    assert_eq!(
        decode_str(&doc_with_attrs(&[two_variants]), &corpus_limits())
            .expect_err("two value variants"),
        OtlpIngestError::ConflictingAttributeValue
    );
    let duplicate_variant =
        r#"{"key":"k","value":{"stringValue":"a","stringValue":"b"}}"#.to_string();
    assert_eq!(
        decode_str(&doc_with_attrs(&[duplicate_variant]), &corpus_limits())
            .expect_err("duplicate value member"),
        OtlpIngestError::ConflictingAttributeValue
    );
}

// --- Required span fields and id validation -------------------------------------------------

#[test]
fn missing_and_malformed_ids_are_typed() {
    let missing_span_id = format!(r#"{{"traceId":"{TRACE_ID}"}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[missing_span_id]), &corpus_limits())
            .expect_err("missing spanId"),
        OtlpIngestError::MissingRequiredSpanField(SpanField::SpanId)
    );
    let short_trace = format!(r#"{{"traceId":"abc","spanId":"{SPAN_ID}"}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[short_trace]), &corpus_limits()).expect_err("short traceId"),
        OtlpIngestError::MalformedSpanField(SpanField::TraceId)
    );
    let non_hex_trace = format!(
        r#"{{"traceId":"g{}","spanId":"{SPAN_ID}"}}"#,
        &TRACE_ID[1..]
    );
    assert_eq!(
        decode_str(&doc_with_spans(&[non_hex_trace]), &corpus_limits())
            .expect_err("non-hex traceId"),
        OtlpIngestError::MalformedSpanField(SpanField::TraceId)
    );
    let short_span = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"abc"}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[short_span]), &corpus_limits()).expect_err("short spanId"),
        OtlpIngestError::MalformedSpanField(SpanField::SpanId)
    );
}

#[test]
fn out_of_range_kind_and_status_are_typed() {
    let bad_kind = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","kind":6}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[bad_kind]), &corpus_limits()).expect_err("kind 6"),
        OtlpIngestError::MalformedSpanField(SpanField::Kind)
    );
    let bad_status =
        format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","status":{{"code":3}}}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[bad_status]), &corpus_limits()).expect_err("status 3"),
        OtlpIngestError::MalformedSpanField(SpanField::StatusCode)
    );
}

#[test]
fn span_without_attributes_or_status_is_ordinary() {
    let span = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}"}}"#);
    let obs = decode_str(&doc_with_spans(&[span]), &corpus_limits()).expect("bare span");
    let span = &obs.spans[0];
    assert_eq!(span.kind, SpanKind::Unspecified);
    assert_eq!(span.method, MethodObservation::Absent);
    assert_eq!(span.operation, OperationObservation::Absent);
    assert_eq!(span.tool_name, None);
    assert_eq!(span.request_id, None);
    assert_eq!(span.protocol_version, SpanProtocolVersion::Absent);
    assert_eq!(span.status, StatusObservation::Absent);
    assert_eq!(span.error_type, ErrorTypeObservation::Absent);
}

#[test]
fn empty_and_absent_resource_spans_are_ordinary() {
    assert!(decode_str("{}", &corpus_limits())
        .expect("absent resourceSpans")
        .spans
        .is_empty());
    assert!(decode_str(r#"{"resourceSpans":[]}"#, &corpus_limits())
        .expect("empty resourceSpans")
        .spans
        .is_empty());
}

// --- Recognized attribute semantics ---------------------------------------------------------

#[test]
fn unrecognized_method_and_operation_are_value_free() {
    let doc = doc_with_attrs(&[
        attr_str("mcp.method.name", "resources/read"),
        attr_str("gen_ai.operation.name", "chat"),
    ]);
    let obs = decode_str(&doc, &corpus_limits()).expect("unrecognized names are observations");
    assert_eq!(obs.spans[0].method, MethodObservation::OtherMethod);
    assert_eq!(obs.spans[0].operation, OperationObservation::OtherOperation);
}

#[test]
fn recognized_attribute_wrong_value_type_rejects_typed() {
    let cases: &[(&str, &str, RecognizedAttribute)] = &[
        (
            "mcp.method.name",
            r#"{"intValue":"1"}"#,
            RecognizedAttribute::MethodName,
        ),
        (
            "gen_ai.operation.name",
            r#"{"boolValue":true}"#,
            RecognizedAttribute::OperationName,
        ),
        (
            "gen_ai.tool.name",
            r#"{"intValue":"7"}"#,
            RecognizedAttribute::ToolName,
        ),
        (
            "jsonrpc.request.id",
            r#"{"doubleValue":1.5}"#,
            RecognizedAttribute::RequestId,
        ),
        (
            "error.type",
            r#"{"boolValue":false}"#,
            RecognizedAttribute::ErrorType,
        ),
    ];
    for (key, value, attribute) in cases {
        let entry = format!(r#"{{"key":"{key}","value":{value}}}"#);
        assert_eq!(
            decode_str(&doc_with_attrs(&[entry]), &corpus_limits()).expect_err("wrong type"),
            OtlpIngestError::RecognizedAttributeWrongType(*attribute),
            "attribute {attribute:?}"
        );
    }
}

#[test]
fn integer_request_id_preserves_upstream_type() {
    let entry = r#"{"key":"jsonrpc.request.id","value":{"intValue":"42"}}"#.to_string();
    let obs = decode_str(&doc_with_attrs(&[entry]), &corpus_limits()).expect("int request id");
    assert_eq!(
        obs.spans[0].request_id,
        Some(RequestIdObservation::Integer(42))
    );
}

#[test]
fn error_status_and_error_type_are_extracted() {
    let span = format!(
        r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","attributes":[{}],"status":{{"code":2}}}}"#,
        attr_str("error.type", "timeout"),
    );
    let obs = decode_str(&doc_with_spans(&[span]), &corpus_limits()).expect("error span");
    assert_eq!(obs.spans[0].status, StatusObservation::Error);
    assert_eq!(
        obs.spans[0].error_type,
        ErrorTypeObservation::Present("timeout".into())
    );
}

// --- Span protocol version: separate observation, never an era ------------------------------

#[test]
fn protocol_version_states_are_typed_observations_not_failures() {
    let cases: &[(&str, SpanProtocolVersion)] = &[
        (
            "2024-11-05",
            SpanProtocolVersion::PresentSupported("2024-11-05".into()),
        ),
        (
            "2031-01-01",
            SpanProtocolVersion::PresentUnsupported("2031-01-01".into()),
        ),
        ("not-a-date", SpanProtocolVersion::Malformed),
        ("2026-02-31", SpanProtocolVersion::Malformed),
    ];
    for (value, expected) in cases {
        let doc = doc_with_attrs(&[attr_str("mcp.protocol.version", value)]);
        let obs = decode_str(&doc, &corpus_limits()).expect("version states never fail decode");
        assert_eq!(&obs.spans[0].protocol_version, expected, "value {value:?}");
    }
}

#[test]
fn non_string_protocol_version_is_the_malformed_state() {
    let entry = r#"{"key":"mcp.protocol.version","value":{"intValue":"5"}}"#.to_string();
    let obs = decode_str(&doc_with_attrs(&[entry]), &corpus_limits()).expect("typed state");
    assert_eq!(
        obs.spans[0].protocol_version,
        SpanProtocolVersion::Malformed
    );
}

#[test]
fn span_protocol_version_has_no_conflict_state() {
    // Exhaustive match: if a Conflicting-style variant is ever added, this stops compiling and
    // forces the boundary discussion in review. A span cannot manufacture the MCP-defined
    // header/body conflict, which belongs to the two-source transport contract only.
    let witness = SpanProtocolVersion::Absent;
    match witness {
        SpanProtocolVersion::Absent
        | SpanProtocolVersion::Malformed
        | SpanProtocolVersion::PresentSupported(_)
        | SpanProtocolVersion::PresentUnsupported(_) => {}
    }
}

// --- No attacker-controlled content in any rejection ----------------------------------------

#[test]
fn rejection_errors_never_echo_attacker_content() {
    const MARKER: &str = "ZZ_ATTACKER_MARKER_ZZ";
    let mut limits = corpus_limits();
    limits.max_attribute_value_bytes = 8;
    limits.max_attribute_key_bytes = 30;
    let hostile_docs = vec![
        // Oversized value carrying the marker.
        doc_with_attrs(&[attr_str("k", &format!("{MARKER}{MARKER}"))]),
        // Duplicate attribute key carrying the marker.
        doc_with_attrs(&[
            attr_str(&format!("a{MARKER}"), "v"),
            attr_str(&format!("a{MARKER}"), "v"),
        ]),
        // Wrong-typed recognized attribute whose sibling carries the marker.
        doc_with_attrs(&[
            attr_str("x", MARKER),
            r#"{"key":"gen_ai.tool.name","value":{"intValue":"1"}}"#.to_string(),
        ]),
        // Malformed id carrying the marker.
        doc_with_spans(&[format!(r#"{{"traceId":"{MARKER}","spanId":"{SPAN_ID}"}}"#)]),
        // Malformed JSON with the marker as the offending token.
        format!(r#"{{"resourceSpans":[]}} {MARKER}"#),
        // Shape fault where the marker is the misplaced value.
        format!(r#"{{"resourceSpans":"{MARKER}"}}"#),
    ];
    for doc in hostile_docs {
        let err = decode_str(&doc, &limits).expect_err("hostile input must reject");
        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(!display.contains(MARKER), "display echoes input: {display}");
        assert!(!debug.contains(MARKER), "debug echoes input: {debug}");
        assert!(
            !display.contains("ATTACKER"),
            "display echoes input fragment"
        );
    }
}

// --- Legacy path stays untouched ------------------------------------------------------------

#[test]
fn legacy_otel_ingest_api_remains_compatible() {
    // Compile-time witness that the pre-existing trace::otel_ingest surface this crate already
    // exposes is unchanged by Slice B. Behavior is covered by its own existing tests.
    let span = crate::trace::otel_ingest::OtelSpan {
        trace_id: "t".into(),
        span_id: "s".into(),
        parent_span_id: None,
        name: "n".into(),
        start_time_unix_nano: "1".into(),
        end_time_unix_nano: "2".into(),
        attributes: None,
    };
    let events = crate::trace::otel_ingest::convert_spans_to_episodes(vec![span]);
    assert!(!events.is_empty());
}

// --- Exact-head review fixes (PR #1944) -----------------------------------------------------

#[test]
fn int_value_string_charges_its_observed_length() {
    // A string-encoded int64 is a JSON string: it charges its UTF-8 length (19 here), never a
    // fixed numeric width.
    let big = "9223372036854775807";
    let entry = format!(r#"{{"key":"k","value":{{"intValue":"{big}"}}}}"#);
    let mut limits = corpus_limits();
    limits.max_attribute_value_bytes = big.len() as u64;
    decode_str(&doc_with_attrs(std::slice::from_ref(&entry)), &limits)
        .expect("exactly at value ceiling");
    limits.max_attribute_value_bytes = big.len() as u64 - 1;
    assert_limit(
        decode_str(&doc_with_attrs(&[entry]), &limits),
        OtlpLimitDimension::AttributeValueBytes,
        big.len() as u64 - 1,
    );
}

#[test]
fn duplicate_member_in_skipped_container_rejects() {
    assert_eq!(
        decode_str(
            r#"{"resourceSpans":[],"unknown":{"same":1,"same":2}}"#,
            &corpus_limits()
        )
        .expect_err("duplicate member inside a skipped map"),
        OtlpIngestError::DuplicateField(ShapeSite::SkippedContainer)
    );
    // The same rule applies arbitrarily deep inside skipped content.
    let span = format!(r#"{{"traceId":"{TRACE_ID}","spanId":"{SPAN_ID}","x":[{{"d":1,"d":2}}]}}"#);
    assert_eq!(
        decode_str(&doc_with_spans(&[span]), &corpus_limits())
            .expect_err("nested duplicate inside skipped content"),
        OtlpIngestError::DuplicateField(ShapeSite::SkippedContainer)
    );
}

#[test]
fn non_string_attribute_key_is_a_typed_entry_shape_fault() {
    let entry = r#"{"key":5,"value":{"stringValue":"v"}}"#.to_string();
    assert_eq!(
        decode_str(&doc_with_attrs(&[entry]), &corpus_limits()).expect_err("non-string key"),
        OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)
    );
}

#[test]
fn uppercase_hex_ids_are_accepted_and_normalized_to_lowercase() {
    // OTLP JSON hex ids are case-insensitive; retained ids normalize to lowercase so one span
    // never splits into two identities by case.
    let span = format!(
        r#"{{"traceId":"{}","spanId":"{}"}}"#,
        TRACE_ID.to_uppercase(),
        SPAN_ID.to_uppercase()
    );
    let obs = decode_str(&doc_with_spans(&[span]), &corpus_limits()).expect("uppercase hex");
    assert_eq!(obs.spans[0].trace_id, TRACE_ID);
    assert_eq!(obs.spans[0].span_id, SPAN_ID);
}

#[test]
fn attribute_count_ceiling_is_per_list() {
    let mut limits = corpus_limits();
    limits.max_attribute_count = 2;
    // Two spans, each exactly at the ceiling; a shared counter would reject the second list.
    let first = span_with_attrs(&[attr_str("a", "v"), attr_str("b", "v")]);
    let second = span_with_attrs(&[attr_str("c", "v"), attr_str("d", "v")]);
    decode_str(&doc_with_spans(&[first, second]), &limits)
        .expect("each span list is counted on its own");
    // The resource list and a span list are independent as well.
    let doc = format!(
        r#"{{"resourceSpans":[{{"resource":{{"attributes":[{},{}]}},"scopeSpans":[{{"spans":[{}]}}]}}]}}"#,
        attr_str("e", "v"),
        attr_str("f", "v"),
        span_with_attrs(&[attr_str("g", "v"), attr_str("h", "v")]),
    );
    decode_str(&doc, &limits).expect("resource and span lists are counted independently");
}
