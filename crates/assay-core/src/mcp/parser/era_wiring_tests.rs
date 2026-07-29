//! Parser-level wiring for the era axes. The pure functions are tested in `mcp::era`; these prove
//! the signals actually reach them from real transcript shapes, which is the half that a
//! unit-green implementation can still get wrong.

use super::*;
use crate::mcp::era::{
    conclude, EnvelopeObservation, EraResolution, IncompleteReason, InvalidReason, ParsedMcpEvent,
    RequestMetadata, RequestMetadata::*, ResultConclusion, ResultObservation,
};
use serde_json::{json, Value};

const V2026: &str = "2026-07-28";
const V2025: &str = "2025-06-18";

fn detailed(input: &str, format: McpInputFormat) -> Vec<ParsedMcpEvent> {
    parse_mcp_transcript_detailed(input, format).expect("transcript parses")
}

fn req(meta: Option<Value>) -> Value {
    let mut params = json!({"name": "Calculator", "arguments": {}});
    if let Some(m) = meta {
        params["_meta"] = m;
    }
    json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call", "params": params})
}

fn meta(version: Value) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion": version})
}

/// `transport_context` is a free-form `Value`, so every shape below is a real transcript.
fn framed(ctx: Option<Value>, entry_ctx: Option<Value>, message: Value) -> String {
    let key = if message.get("result").is_some() {
        "response"
    } else {
        "request"
    };
    let mut entry = json!({"timestamp_ms": 1000, key: message});
    if let Some(c) = entry_ctx {
        entry["transport_context"] = c;
    }
    let mut doc = json!({"transport": "streamable-http", "entries": [entry]});
    if let Some(c) = ctx {
        doc["transport_context"] = c;
    }
    doc.to_string()
}

fn headers(version: Value) -> Value {
    json!({"headers": {"MCP-Protocol-Version": version}})
}

// --- Envelope, read from both levels -----------------------------------------------------------

/// The existing fixtures carry the header at transcript level, so this is the shape that must work
/// or the whole corpus reads as `Absent`.
#[test]
fn a_transcript_level_header_is_observed() {
    let input = framed(Some(headers(json!(V2026))), None, req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Present(V2026.into())
    );
}

#[test]
fn an_entry_level_header_is_observed() {
    let input = framed(None, Some(headers(json!(V2026))), req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Present(V2026.into())
    );
}

#[test]
fn two_levels_agreeing_are_present() {
    let input = framed(
        Some(headers(json!(V2026))),
        Some(headers(json!(V2026))),
        req(None),
    );
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Present(V2026.into())
    );
}

/// Fail-closed. Two levels of the same transcript stating different versions is not a signal to
/// pick from, and `Malformed` is enough for this slice: it blocks a conclusion without inventing a
/// resolution rule the spec does not give.
#[test]
fn two_levels_disagreeing_are_malformed() {
    let input = framed(
        Some(headers(json!(V2025))),
        Some(headers(json!(V2026))),
        req(None),
    );
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Malformed
    );
}

/// The reason the existing header helper cannot be reused unchanged: it turns a non-string into
/// `None`, which would report silence where a signal arrived and failed.
#[test]
fn a_non_string_header_is_malformed_not_absent() {
    for bad in [
        json!(2026),
        json!({"v": V2026}),
        json!([V2026]),
        json!(null),
    ] {
        let input = framed(Some(headers(bad.clone())), None, req(None));
        assert_eq!(
            detailed(&input, McpInputFormat::StreamableHttp)[0]
                .context
                .envelope,
            EnvelopeObservation::Malformed,
            "{bad}"
        );
    }
}

#[test]
fn an_empty_header_is_malformed() {
    let input = framed(Some(headers(json!(""))), None, req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Malformed
    );
}

#[test]
fn a_framed_transcript_with_no_header_is_absent() {
    let input = framed(None, None, req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Absent
    );
}

#[test]
fn unframed_formats_are_not_applicable() {
    for (input, format) in [
        (req(None).to_string(), McpInputFormat::JsonRpc),
        (
            json!({"events": [req(None)]}).to_string(),
            McpInputFormat::Inspector,
        ),
    ] {
        assert_eq!(
            detailed(&input, format)[0].context.envelope,
            EnvelopeObservation::NotApplicable
        );
    }
}

// --- Request metadata --------------------------------------------------------------------------

#[test]
fn a_request_metadata_version_is_read_from_params_meta() {
    let input = req(Some(meta(json!(V2026)))).to_string();
    assert_eq!(
        detailed(&input, McpInputFormat::JsonRpc)[0]
            .context
            .request_metadata,
        Some(Present(V2026.into()))
    );
}

#[test]
fn a_request_without_meta_reports_absent_metadata() {
    let input = req(None).to_string();
    assert_eq!(
        detailed(&input, McpInputFormat::JsonRpc)[0]
            .context
            .request_metadata,
        Some(Absent)
    );
}

#[test]
fn a_non_string_metadata_version_is_malformed() {
    for bad in [json!(7), json!({}), json!([]), json!("")] {
        let input = req(Some(meta(bad.clone()))).to_string();
        assert_eq!(
            detailed(&input, McpInputFormat::JsonRpc)[0]
                .context
                .request_metadata,
            Some(RequestMetadata::Malformed),
            "{bad}"
        );
    }
}

// --- Result observation --------------------------------------------------------------------------

fn response(result_type: Option<Value>) -> Value {
    let mut r = json!({"content": []});
    if let Some(t) = result_type {
        r["resultType"] = t;
    }
    json!({"jsonrpc": "2.0", "id": "call-1", "result": r})
}

fn observed(result_type: Option<Value>) -> ResultObservation {
    let input = framed(Some(headers(json!(V2026))), None, response(result_type));
    detailed(&input, McpInputFormat::StreamableHttp)
        .into_iter()
        .find_map(|e| e.context.result_observation)
        .expect("a response event")
}

#[test]
fn every_result_type_shape_reaches_the_axis() {
    assert_eq!(observed(None), ResultObservation::Missing);
    assert_eq!(
        observed(Some(json!("complete"))),
        ResultObservation::Complete
    );
    assert_eq!(
        observed(Some(json!("input_required"))),
        ResultObservation::InputRequired
    );
    assert_eq!(
        observed(Some(json!("banana"))),
        ResultObservation::Unrecognized
    );
}

/// The shapes that must not become `Missing` or a token, since either would let an unreadable
/// field inherit the absent-means-complete rule.
#[test]
fn a_non_string_result_type_is_malformed() {
    for bad in [json!(1), json!({}), json!([]), json!(null)] {
        assert_eq!(
            observed(Some(bad.clone())),
            ResultObservation::Malformed,
            "{bad}"
        );
    }
}

/// A request has no result to observe.
#[test]
fn a_request_has_no_result_observation() {
    let input = framed(Some(headers(json!(V2026))), None, req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .result_observation,
        None
    );
}

// --- The parser does not refuse ----------------------------------------------------------------

/// Every malformed shape above is an observable protocol defect, not an unparsable transcript.
/// Refusing here would conflate the two and would break `mcp_import_smoke.rs`.
#[test]
fn malformed_observations_never_fail_the_parse() {
    let cases = [
        framed(Some(headers(json!(2026))), None, req(None)),
        framed(
            Some(headers(json!(V2025))),
            Some(headers(json!(V2026))),
            req(None),
        ),
        req(Some(meta(json!(7)))).to_string(),
        framed(Some(headers(json!(V2026))), None, response(Some(json!(1)))),
    ];
    for (i, input) in cases.iter().enumerate() {
        let format = if input.contains("\"transport\"") {
            McpInputFormat::StreamableHttp
        } else {
            McpInputFormat::JsonRpc
        };
        assert!(
            parse_mcp_transcript_detailed(input, format).is_ok(),
            "case {i}"
        );
        assert!(parse_mcp_transcript(input, format).is_ok(), "case {i}");
    }
}

// --- Per-entry scope, and the slots that were being ignored -------------------------------------

/// A later deviant entry must not reach back and contaminate an earlier correct one. The first
/// implementation folded every entry into one transcript-wide observation and copied it to all.
#[test]
fn a_deviant_entry_does_not_contaminate_the_others() {
    let doc = json!({
        "transport": "streamable-http",
        "entries": [
            {"timestamp_ms": 1000, "request": req(None)},
            {"timestamp_ms": 1001, "transport_context": headers(json!(2026)),
             "request": {"jsonrpc": "2.0", "id": "call-2", "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {}}}}
        ]
    });
    let mut ctx = json!({"headers": {"MCP-Protocol-Version": V2026}});
    let mut doc = doc;
    doc["transport_context"] = std::mem::take(&mut ctx);
    let parsed = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    assert_eq!(
        parsed[0].context.envelope,
        EnvelopeObservation::Present(V2026.into()),
        "the clean entry keeps the transcript default"
    );
    assert_eq!(
        parsed[1].context.envelope,
        EnvelopeObservation::Malformed,
        "only the deviant entry is malformed"
    );
}

/// The direct `headers` field exists on both the transcript and each entry, beside the nested
/// `transport_context.headers`. Reading only the nested one drops half the surface.
#[test]
fn a_direct_headers_field_is_read_at_both_levels() {
    for doc in [
        json!({"transport": "streamable-http",
               "headers": {"MCP-Protocol-Version": V2026},
               "entries": [{"timestamp_ms": 1000, "request": req(None)}]}),
        json!({"transport": "streamable-http",
               "entries": [{"timestamp_ms": 1000,
                            "headers": {"MCP-Protocol-Version": V2026},
                            "request": req(None)}]}),
    ] {
        assert_eq!(
            detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
                .context
                .envelope,
            EnvelopeObservation::Present(V2026.into())
        );
    }
}

/// Two spellings of one header is not a value to choose between.
#[test]
fn duplicate_case_variants_of_the_header_are_malformed() {
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {
            "MCP-Protocol-Version": V2026,
            "mcp-protocol-version": V2025
        }},
        "entries": [{"timestamp_ms": 1000, "request": req(None)}]
    });
    assert_eq!(
        detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Malformed
    );
}

/// A response has no request metadata to observe, so the field is absent rather than reported as
/// an absent version. The comment said so before the code did.
#[test]
fn a_response_reports_no_request_metadata() {
    let input = framed(Some(headers(json!(V2026))), None, response(None));
    let parsed = detailed(&input, McpInputFormat::StreamableHttp);
    let response_event = parsed
        .iter()
        .find(|e| e.context.result_observation.is_some())
        .expect("a response event");
    assert_eq!(response_event.context.request_metadata, None);
}

/// `_meta` is an object by schema. A scalar there is a signal that arrived and failed, and
/// `Value::get` on a non-object answers `None`, which would have reported it as silence.
#[test]
fn a_non_object_meta_is_malformed() {
    for bad in [json!("2026-07-28"), json!(7), json!([]), json!(null)] {
        let input = req(Some(bad.clone())).to_string();
        assert_eq!(
            detailed(&input, McpInputFormat::JsonRpc)[0]
                .context
                .request_metadata,
            Some(RequestMetadata::Malformed),
            "{bad}"
        );
    }
}

/// A malformed hybrid: a valid string `method` alongside a `result`. The parser calls this a
/// request, so the metadata observation has to follow the same rule. Keying on the absence of
/// `result` would drop the observation for exactly the shape worth observing.
#[test]
fn a_hybrid_with_method_and_result_still_reports_request_metadata() {
    let hybrid = json!({
        "jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
        "params": {"name": "Calculator", "arguments": {}, "_meta": meta(json!(V2026))},
        "result": {"content": []}
    });
    assert_eq!(
        detailed(&hybrid.to_string(), McpInputFormat::JsonRpc)[0]
            .context
            .request_metadata,
        Some(Present(V2026.into()))
    );
}

/// Two spellings of the header carrying the same value agree, and agreement is not a choice. An
/// earlier version failed closed on the duplicate itself rather than on a disagreement.
#[test]
fn duplicate_case_variants_agreeing_are_present() {
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {
            "MCP-Protocol-Version": V2026,
            "mcp-protocol-version": V2026
        }},
        "entries": [{"timestamp_ms": 1000, "request": req(None)}]
    });
    assert_eq!(
        detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Present(V2026.into())
    );
}

/// An oversized header value must not be retained, let alone once per entry. An observation only
/// ever holds a value it has already accepted as a version, so a rejected one costs the enum
/// discriminant and nothing more, however large the input was.
#[test]
fn an_oversized_header_is_not_retained_per_entry() {
    let huge = "x".repeat(1024 * 1024);
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": huge}},
        "entries": [
            {"timestamp_ms": 1000, "request": req(None)},
            {"timestamp_ms": 1001,
             "request": {"jsonrpc": "2.0", "id": "call-2", "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {}}}}
        ]
    });
    let parsed = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    assert_eq!(parsed.len(), 2);
    for (i, event) in parsed.iter().enumerate() {
        assert_eq!(
            event.context.envelope,
            EnvelopeObservation::Malformed,
            "entry {i} must not carry the value"
        );
    }
}

/// The same bound on the body signal.
#[test]
fn an_oversized_metadata_version_is_not_retained() {
    let huge = "x".repeat(1024 * 1024);
    let input = req(Some(meta(json!(huge)))).to_string();
    assert_eq!(
        detailed(&input, McpInputFormat::JsonRpc)[0]
            .context
            .request_metadata,
        Some(RequestMetadata::Malformed)
    );
}

/// A `headers` node that is not an object is a signal that arrived and failed. `as_object`
/// answering `None` would have dropped the slot and reported silence.
#[test]
fn a_non_object_headers_node_is_malformed_not_absent() {
    for doc in [
        json!({"transport": "streamable-http", "headers": 7,
               "entries": [{"timestamp_ms": 1000, "request": req(None)}]}),
        json!({"transport": "streamable-http", "transport_context": {"headers": 7},
               "entries": [{"timestamp_ms": 1000, "request": req(None)}]}),
    ] {
        assert_eq!(
            detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
                .context
                .envelope,
            EnvelopeObservation::Malformed,
            "{doc}"
        );
    }
}

/// A `result` that is not an object cannot be missing a field. Reading it as `Missing` let the
/// backward-compatibility rule turn `"result": null` into a completed action under a legacy era.
#[test]
fn a_non_object_result_is_malformed_not_missing() {
    for bad in [json!(null), json!(7), json!([]), json!("done")] {
        let message = json!({"jsonrpc": "2.0", "id": "call-1", "result": bad});
        let input = framed(Some(headers(json!(V2025))), None, message);
        let observed = detailed(&input, McpInputFormat::StreamableHttp)
            .into_iter()
            .find_map(|e| e.context.result_observation)
            .expect("a response event");
        assert_eq!(observed, ResultObservation::Malformed, "{bad}");
    }
}

/// `ResultType` is an open string union, so an empty string is syntactically a token. Unrecognized
/// rather than unreadable, which is a different finding with a different conclusion.
#[test]
fn an_empty_result_type_is_unrecognized_not_malformed() {
    let input = framed(Some(headers(json!(V2026))), None, response(Some(json!(""))));
    let observed = detailed(&input, McpInputFormat::StreamableHttp)
        .into_iter()
        .find_map(|e| e.context.result_observation)
        .expect("a response event");
    assert_eq!(observed, ResultObservation::Unrecognized);
}

/// The whole chain on the shape that was silently completing: a legacy-era transcript whose
/// response carries `"result": null`. Parser observation through to conclusion, permanently, so
/// this cannot regress through either half alone.
#[test]
fn composite_a_null_result_under_a_legacy_era_is_invalid() {
    let message = json!({"jsonrpc": "2.0", "id": "call-1", "result": null});
    let input = framed(Some(headers(json!(V2025))), None, message);
    let parsed = detailed(&input, McpInputFormat::StreamableHttp);
    let event = parsed
        .iter()
        .find(|e| e.context.result_observation.is_some())
        .expect("a response event");
    assert_eq!(event.context.era, EraResolution::Known(V2025.into()));
    assert_eq!(
        event.context.result_observation,
        Some(ResultObservation::Malformed)
    );
    assert_eq!(
        conclude(
            &event.context.era,
            event.context.result_observation.as_ref().unwrap()
        ),
        ResultConclusion::Invalid(InvalidReason::MalformedResultType)
    );
}

/// An unrecognized token reaches the conclusion without carrying the token.
#[test]
fn composite_an_unknown_token_is_incomplete_and_value_free() {
    let input = framed(
        Some(headers(json!(V2026))),
        None,
        response(Some(json!("banana"))),
    );
    let parsed = detailed(&input, McpInputFormat::StreamableHttp);
    let event = parsed
        .iter()
        .find(|e| e.context.result_observation.is_some())
        .expect("a response event");
    let conclusion = conclude(
        &event.context.era,
        event.context.result_observation.as_ref().unwrap(),
    );
    assert_eq!(
        conclusion,
        ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
    );
    assert!(
        !format!("{conclusion:?}").contains("banana"),
        "the token must not travel: {conclusion:?}"
    );
}
