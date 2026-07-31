//! Parser-level wiring for the era axes. The pure functions are tested in `mcp::era`; these prove
//! the signals actually reach them from real transcript shapes, which is the half that a
//! unit-green implementation can still get wrong.

use super::*;
use crate::mcp::era::{classify_message, conclude_request, RequestAssessment, UnknownReason};
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
    // Which wrapper slot the message belongs in, keyed on the presence of a `method` member and
    // nothing about its value. This is the wrapper's own question, not the classifier's: a present
    // but non-string `method` is a malformed message shape that `classify_message` refuses, and the
    // refusal has to happen on a message the transcript placed as a request. Keying on `as_str` sent
    // exactly that fixture into the response slot, so the malformed-method test was exercising a path
    // its own comment did not describe. `two_entry_doc` keys the same way.
    let key = if message.get("method").is_some() {
        "request"
    } else {
        "response"
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

/// As `observed`, for a result that also carries a continuation member.
fn observed_with_continuation(result_type: Value) -> ResultObservation {
    let mut msg = response(Some(result_type));
    msg["result"]["requestState"] = json!("s1");
    let input = framed(Some(headers(json!(V2026))), None, msg);
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
    // `input_required` reaches two points, not one, and which one depends on whether the result
    // carries a way to continue. `response` builds a bare result, so the bare token is the
    // uncontinuable shape: `InputRequiredResult` requires at least one of `inputRequests` or
    // `requestState` in prose and encodes neither, so this is the shape the schema lets through.
    assert_eq!(
        observed(Some(json!("input_required"))),
        ResultObservation::InputRequiredWithoutContinuation
    );
    assert_eq!(
        observed_with_continuation(json!("input_required")),
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

/// The invariant stated once over every shape that reaches the parser:
/// the two axes are mutually exclusive because one discriminant produces both. A message is a
/// request or a response, never a request for the payload and a response for the sidecar.
#[test]
fn no_event_ever_carries_both_axes() {
    let hybrid_result = json!({
        "jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
        "params": {"name": "Calculator", "arguments": {}, "_meta": meta(json!(V2026))},
        "result": {"content": [], "resultType": "complete"}
    });
    let hybrid_error = json!({
        "jsonrpc": "2.0", "id": "call-2", "method": "tools/list",
        "error": {"code": -32000, "message": "boom"}
    });
    let hybrid_unreadable_result = json!({
        "jsonrpc": "2.0", "id": "call-3", "method": "notifications/initialized",
        "result": null
    });
    // A real notification: a string `method` and no `id`, and not a request-only name. Without it
    // the matrix could not tell "is a request" from "has a method", so the invariant below was
    // satisfied by a rule that is not the one the parser follows.
    let notification = json!({
        "jsonrpc": "2.0", "method": "notifications/progress",
        "params": {"progressToken": "t", "progress": 1}
    });
    for message in [
        req(Some(meta(json!(V2026)))),
        response(Some(json!("complete"))),
        hybrid_result,
        hybrid_error,
        hybrid_unreadable_result,
        notification,
    ] {
        let parsed = detailed(&message.to_string(), McpInputFormat::JsonRpc);
        let context = &parsed[0].context;
        assert!(
            context.request_metadata.is_none() || context.result_observation.is_none(),
            "both axes populated for {message}: {context:?}"
        );
        // The expectation comes from the shared classifier, not from a second reading of the shape.
        // "Has a string method" is not the rule: a method with no `id` is a notification unless the
        // name is request-only, and a notification carries no request-metadata axis at all.
        let expects_request_axis =
            matches!(classify_message(&message), Ok(MessageKind::Request { .. }));
        assert_eq!(
            context.request_metadata.is_some(),
            expects_request_axis,
            "the request axis must follow the payload discriminant for {message}"
        );
    }
}

/// A present non-object `transport_context` was naively read as `Absent`, because `Value::get`
/// returned `None` for it, which is the same answer as no container at all. The fixed path reports
/// `Malformed`, preserving the evidence that framing was present but unreadable.
#[test]
fn a_non_object_transport_context_is_malformed_not_absent() {
    for bad in [json!(7), json!("2026-07-28"), json!([]), json!(null)] {
        let input = framed(Some(bad.clone()), None, req(None));
        assert_eq!(
            detailed(&input, McpInputFormat::StreamableHttp)[0]
                .context
                .envelope,
            EnvelopeObservation::Malformed,
            "{bad}"
        );
    }
}

/// The same shape one level down, where dropping the slot is worse: the entry silently inherits a
/// valid transcript default and its own broken signal disappears.
#[test]
fn a_non_object_entry_transport_context_does_not_inherit_the_transcript_default() {
    for bad in [json!(7), json!("2026-07-28"), json!([]), json!(null)] {
        let input = framed(Some(headers(json!(V2026))), Some(bad.clone()), req(None));
        assert_eq!(
            detailed(&input, McpInputFormat::StreamableHttp)[0]
                .context
                .envelope,
            EnvelopeObservation::Malformed,
            "{bad}"
        );
    }
}

/// The distinction the fix rests on, from the other side: a container that is readable and simply
/// carries no header slot is silence, not a defect. Folding it to `Malformed` would report a
/// finding on every transcript whose `transport_context` holds anything else.
#[test]
fn a_readable_transport_context_without_headers_stays_absent() {
    let input = framed(Some(json!({"status": 200})), None, req(None));
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Absent
    );
}

/// An explicit `null` in a slot is a key that was written, not a key that was omitted. `Option<T>`
/// folds both onto `None`, so the slots are deserialized to keep the difference.
#[test]
fn an_explicit_null_headers_slot_is_malformed_not_absent() {
    let doc = json!({"transport": "streamable-http", "headers": null,
                     "entries": [{"timestamp_ms": 1000, "request": req(None)}]});
    assert_eq!(
        detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Malformed
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
            event.context.result_observation.as_ref().unwrap(),
            None
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
        None,
    );
    // No request is framed here, so the parser leaves `capability_observation: None`. Under a
    // revision that defines the capability set, that cannot reach the closed answer: nothing was
    // ever advertised to this build, so "nothing advertised covers it" has no ground.
    assert_eq!(event.context.capability_observation, None);
    assert_eq!(
        conclusion,
        ResultConclusion::Incomplete(IncompleteReason::RecognitionUndeterminable)
    );
    assert!(
        !format!("{conclusion:?}").contains("banana"),
        "the token must not travel: {conclusion:?}"
    );
}

/// The hybrid fixture, both axes on one message. A valid string `method` makes this a request, so
/// the metadata observation follows that discriminant and no result observation is taken: keying
/// the metadata on the absence of `result` would drop it for exactly the shape worth observing,
/// and reading the `result` anyway would hand slice 2 a result conclusion about a request.
#[test]
fn a_hybrid_reports_request_metadata_and_no_result_observation() {
    let hybrid = json!({
        "jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
        "params": {"name": "Calculator", "arguments": {}, "_meta": meta(json!(V2026))},
        "result": {"content": []}
    });
    let event = &detailed(&hybrid.to_string(), McpInputFormat::JsonRpc)[0];
    assert_eq!(event.context.request_metadata, Some(Present(V2026.into())));
    assert_eq!(event.context.result_observation, None);
}

/// The fourth container. `params` is an object by schema, and reaching through a scalar with
/// `Value::get` answers `None`, which reads a container that arrived and failed as silence.
#[test]
fn a_non_object_params_is_malformed_not_absent() {
    for bad in [
        json!(7),
        json!("x"),
        json!([]),
        json!(null),
        json!([{"_meta": {}}]),
    ] {
        let message = json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
                             "params": bad});
        assert_eq!(
            detailed(&message.to_string(), McpInputFormat::JsonRpc)[0]
                .context
                .request_metadata,
            Some(RequestMetadata::Malformed),
            "{bad}"
        );
    }
}

/// The whole chain under a legacy era, which is where the difference decides the verdict. `Absent`
/// is only a fault from 2026 on, so a deviant container came back `Valid`: no objection recorded
/// against a request whose parameters could not be read.
#[test]
fn composite_a_deviant_params_under_a_legacy_era_is_invalid() {
    let message = json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call", "params": 7});
    let input = framed(Some(headers(json!(V2025))), None, message);
    let event = &detailed(&input, McpInputFormat::StreamableHttp)[0];
    assert_eq!(
        event.context.era,
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
    let observed = event
        .context
        .request_metadata
        .as_ref()
        .expect("a request reports metadata");
    assert_eq!(observed, &RequestMetadata::Malformed);
    assert_eq!(
        conclude_request(
            &event.context.era,
            observed,
            Some(&CapabilityObservation::CoreOnly)
        ),
        RequestAssessment::Invalid(InvalidReason::MalformedRequestMetadata)
    );
}

/// The same chain at 2026, so the rule is not satisfied by the era alone.
#[test]
fn composite_a_deviant_params_at_2026_is_invalid_for_the_container_not_the_era() {
    let message = json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call", "params": []});
    let input = framed(Some(headers(json!(V2026))), None, message);
    let event = &detailed(&input, McpInputFormat::StreamableHttp)[0];
    // Read from the context rather than handed in: feeding `Malformed` here would keep the
    // assertion green after the guard that produces it was removed, which is the whole thing this
    // composite exists to catch.
    let observed = event
        .context
        .request_metadata
        .as_ref()
        .expect("a request reports metadata");
    assert_eq!(observed, &RequestMetadata::Malformed);
    assert_eq!(
        conclude_request(
            &event.context.era,
            observed,
            Some(&CapabilityObservation::CoreOnly)
        ),
        RequestAssessment::Invalid(InvalidReason::MalformedRequestMetadata)
    );
}

/// An entry that writes its own `headers` slot as an explicit `null` must not inherit the valid
/// transcript default. Mutation showed this was the one of the four `present_slot` sites no test
/// pinned, and it is the site where dropping the slot silently borrows another entry's evidence.
#[test]
fn an_explicit_null_entry_headers_slot_does_not_inherit_the_transcript_default() {
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2026}},
        "entries": [{"timestamp_ms": 1000, "headers": null, "request": req(None)}]
    });
    assert_eq!(
        detailed(&doc.to_string(), McpInputFormat::StreamableHttp)[0]
            .context
            .envelope,
        EnvelopeObservation::Malformed
    );
}

/// A real notification under 2026 with no `_meta`. `NotificationParams._meta` is optional and is a
/// different type that does not carry the protocol version, so the request requirement does not
/// apply and reporting it would invent a fault.
#[test]
fn a_notification_is_not_held_to_the_request_metadata_requirement() {
    let notification = json!({"jsonrpc": "2.0", "method": "notifications/progress",
                              "params": {"progressToken": "t", "progress": 1}});
    let input = framed(Some(headers(json!(V2026))), None, notification);
    let event = &detailed(&input, McpInputFormat::StreamableHttp)[0];
    assert_eq!(event.context.era, EraResolution::Known(V2026.into()));
    assert_eq!(
        event.context.request_metadata, None,
        "a notification has no request-metadata axis"
    );
    assert_eq!(event.context.result_observation, None);
}

/// A notification is a request object *without* an `id` member, so absence is the discriminant and
/// the value is not. An explicit `"id": null` is therefore a request shape, and because a request
/// must name the call it belongs to, an unusable id is refused rather than quietly downgraded to a
/// notification. Downgrading would have dropped the required 2026 metadata check for any message
/// that writes one token.
#[test]
fn an_explicit_null_id_is_a_refused_request_not_a_notification() {
    let message = json!({"jsonrpc": "2.0", "id": null, "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {}}});
    let input = framed(Some(headers(json!(V2026))), None, message);
    let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp)
        .expect_err("a request with a null id is refused");
    assert!(
        format!("{err:?}").contains("must be a string or a number"),
        "{err:?}"
    );
    // The same token on a notification is not a refusal, which is what makes this a statement about
    // request shape rather than about `null`.
    let notification = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}});
    assert!(parse_mcp_transcript(
        &framed(Some(headers(json!(V2026))), None, notification),
        McpInputFormat::StreamableHttp
    )
    .is_ok());
}

/// The positive control: a request still reports the axis, so the notification rule did not turn it
/// off for everything with a `method`.
#[test]
fn a_request_still_reports_the_metadata_axis() {
    let input = framed(
        Some(headers(json!(V2026))),
        None,
        req(Some(meta(json!(V2026)))),
    );
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .request_metadata,
        Some(Present(V2026.into()))
    );
}

// --- Message shape: one classifier, and the shapes it refuses ----------------------------------

/// A present non-string `method` is a malformed message shape, not a message of another kind.
/// Folding it through `as_str()` answered `Response`, which dropped the required 2026
/// request-metadata check for anything that writes one wrong-typed field. The refusal is a JSON-RPC
/// shape refusal rather than an era-state refusal, so it does not weaken the rule that every era
/// observation parses.
#[test]
fn a_non_string_method_is_refused_at_parse_time() {
    for bad in [json!(7), json!({}), json!([]), json!(null), json!(true)] {
        let message = json!({"jsonrpc": "2.0", "id": "call-1", "method": bad,
                             "params": {"name": "Calculator", "arguments": {}}});
        let input = framed(Some(headers(json!(V2026))), None, message);
        let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp)
            .expect_err(&format!("must refuse method {bad}"));
        // Split the located prefix from the diagnostic and assert each half for what it is.
        // Searching the whole string for the offending value cannot work: `!contains("7")` also
        // matches a source line numbered 7, and for `{}`, `[]`, `null` the search has nothing to
        // find, so it passes while proving nothing. Pinning the diagnostic to one exact constant is
        // the value-free property itself, since a constant cannot carry an input-chosen value.
        let rendered = err.to_string();
        let (prefix, diagnostic) = rendered
            .split_once(": ")
            .unwrap_or_else(|| panic!("expected a located diagnostic for {bad}: {rendered}"));
        let line = prefix
            .strip_prefix("MCP event at source line ")
            .unwrap_or_else(|| panic!("unexpected prefix for {bad}: {prefix}"));
        assert!(
            !line.is_empty() && line.bytes().all(|c| c.is_ascii_digit()),
            "source line is not a number for {bad}: {line}"
        );
        assert_eq!(
            diagnostic, "JSON-RPC method must be a string",
            "the diagnostic must be exactly this constant, for {bad}"
        );
    }
}

/// The controls, so the refusal did not become a blanket rule. Each valid shape parses and lands on
/// exactly the axes its kind licenses.
#[test]
fn each_valid_shape_lands_on_its_own_axes() {
    let request = json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {}}});
    let notification = json!({"jsonrpc": "2.0", "method": "notifications/progress",
                              "params": {"progressToken": "t", "progress": 1}});
    let response = json!({"jsonrpc": "2.0", "id": "call-1",
                          "result": {"content": [], "resultType": "complete"}});
    for (label, message, meta_axis, result_axis) in [
        ("request", request, true, false),
        ("notification", notification, false, false),
        ("response", response, false, true),
    ] {
        let input = framed(Some(headers(json!(V2026))), None, message);
        let event = &detailed(&input, McpInputFormat::StreamableHttp)[0];
        assert_eq!(
            event.context.request_metadata.is_some(),
            meta_axis,
            "{label} metadata axis"
        );
        assert_eq!(
            event.context.result_observation.is_some(),
            result_axis,
            "{label} result axis"
        );
    }
}

/// The parser and the sidecar must not merely agree today, they must be reading the same answer.
/// Two callers each reaching for `method` with their own `as_str()` is how the discriminants drifted
/// apart before, so this asserts the axis assignment against `classify_message` itself rather than
/// against a second copy of the rule.
#[test]
fn the_sidecar_axes_follow_the_shared_classifier() {
    let shapes = [
        json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
               "params": {"name": "Calculator", "arguments": {}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}),
        json!({"jsonrpc": "2.0", "id": "call-1", "method": "", "params": {}}),
        json!({"jsonrpc": "2.0", "id": "call-1", "result": {"content": [], "resultType": "complete"}}),
        json!({"jsonrpc": "2.0", "id": "call-1", "error": {"code": -1, "message": "x"}}),
        // A no-method object with neither `result` nor `error` is refused as a message shape now,
        // so it is not a classifiable input and belongs in the refusal test instead.
    ];
    for shape in shapes {
        let expected = classify_message(&shape).expect("a classifiable shape");
        let input = framed(Some(headers(json!(V2026))), None, shape.clone());
        let event = &detailed(&input, McpInputFormat::StreamableHttp)[0];
        let (meta, result) = (
            event.context.request_metadata.is_some(),
            event.context.result_observation.is_some(),
        );
        match expected {
            MessageKind::Request { .. } => assert_eq!((meta, result), (true, false), "{shape}"),
            MessageKind::Notification { .. } => {
                assert_eq!((meta, result), (false, false), "{shape}")
            }
            // An error response has no `result` to observe, so the result axis is licensed but
            // empty; what matters is that the metadata axis stays off.
            MessageKind::Response => assert!(!meta, "{shape}"),
        }
    }
}

// --- Call correlation: a response inherits its call's era ---------------------------------------

/// The gap this closes. The era resolves from signals that live on a request, so a response carries
/// none of its own and falls back to the transcript header. A request whose header and body
/// disagree is `Conflicting`, but its response resolved to `Known(2025)` from the header alone, and
/// a missing `resultType` at 2025 is `Terminal`. So a contradicted call could still conclude that
/// the action completed. The request's era must reach the correlated response.
#[test]
fn a_response_inherits_the_conflicting_era_of_its_call() {
    let request = json!({"jsonrpc": "2.0", "id": "call-1", "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {},
                                    "_meta": meta(json!(V2026))}});
    let response = json!({"jsonrpc": "2.0", "id": "call-1", "result": {"content": []}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "request": request},
            {"timestamp_ms": 1001, "response": response}
        ]
    });
    let events = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    let req_event = &events[0];
    let resp_event = &events[1];
    assert_eq!(
        req_event.context.era,
        EraResolution::Conflicting {
            header: V2025.into(),
            body: V2026.into()
        },
        "the request itself is contradicted"
    );
    assert_eq!(
        resp_event.context.era,
        EraResolution::Conflicting {
            header: V2025.into(),
            body: V2026.into()
        },
        "the response must inherit its call's era"
    );
    let observed = resp_event
        .context
        .result_observation
        .as_ref()
        .expect("a response reports a result");
    assert_eq!(observed, &ResultObservation::Missing);
    assert_eq!(
        conclude(&resp_event.context.era, observed, None),
        ResultConclusion::Invalid(InvalidReason::EraConflicting {
            header: V2025.into(),
            body: V2026.into()
        }),
        "a contradicted call cannot conclude that the action completed"
    );
}

/// A response with no matching request keeps the era it resolved on its own, so correlation adds
/// authority rather than removing it.
#[test]
fn an_uncorrelated_response_keeps_its_own_era() {
    let response = json!({"jsonrpc": "2.0", "id": "orphan", "result": {"content": []}});
    let input = framed(Some(headers(json!(V2025))), None, response);
    assert_eq!(
        detailed(&input, McpInputFormat::StreamableHttp)[0]
            .context
            .era,
        EraResolution::Known(V2025.into())
    );
}

// --- Duplicate members ---------------------------------------------------------------------------

/// `serde_json` collapses duplicate members last-value-wins, so by the time any guard reads the
/// tree the evidence that two were sent is gone. Refused at the boundary instead, value-free.
#[test]
fn duplicate_members_are_refused_at_the_boundary() {
    let cases = [
        (
            "resultType",
            r#"{"transport":"streamable-http","entries":[{"response":{"jsonrpc":"2.0","id":"c","result":{"content":[],"resultType":"complete","resultType":"input_required"}}}]}"#,
        ),
        (
            "params._meta protocolVersion",
            r#"{"transport":"streamable-http","entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2025-06-18","io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}}]}"#,
        ),
        (
            "MCP-Protocol-Version header",
            r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2025-06-18","MCP-Protocol-Version":"2026-07-28"}},"entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#,
        ),
    ];
    for (label, raw) in cases {
        let err = parse_mcp_transcript(raw, McpInputFormat::StreamableHttp)
            .expect_err(&format!("must refuse a duplicate {label}"));
        // The whole chain, because `main` prints `{e:?}` and that is what an operator sees. The
        // outermost context names the input; the cause names why.
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("duplicate member"),
            "unexpected message for {label}: {rendered}"
        );
        assert!(
            !rendered.contains("2026-07-28") && !rendered.contains("input_required"),
            "refusal echoed an input value for {label}: {rendered}"
        );
    }
}

/// A duplicate on a bare JSON-RPC line is refused on the same basis.
#[test]
fn a_duplicate_member_on_a_jsonrpc_line_is_refused() {
    let line =
        r#"{"jsonrpc":"2.0","id":"c","method":"tools/call","method":"tools/list","params":{}}"#;
    let err = parse_mcp_transcript(line, McpInputFormat::JsonRpc).expect_err("must refuse");
    assert!(format!("{err:?}").contains("duplicate member"), "{err:?}");
}

/// Distinct members that merely look alike are not duplicates, so the rule does not become a ban on
/// similar keys.
#[test]
fn distinct_members_are_not_duplicates() {
    let raw = r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28","X-Other":"2025-06-18"}},"entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#;
    assert!(parse_mcp_transcript(raw, McpInputFormat::StreamableHttp).is_ok());
}

// --- SSE data goes through the same boundary ----------------------------------------------------

/// `TransportSseEnvelope.data` was a plain `Value`, so an SSE frame bypassed the duplicate-aware
/// boundary entirely: both the structured form and the JSON-string form kept only the last member.
#[test]
fn duplicate_members_in_sse_data_are_refused() {
    let structured = r#"{"transport":"http-sse","entries":[{"sse":{"event":"message","data":{"jsonrpc":"2.0","id":"c","result":{"content":[],"resultType":"complete","resultType":"input_required"}}}}]}"#;
    let as_string = r#"{"transport":"http-sse","entries":[{"sse":{"event":"message","data":"{\"jsonrpc\":\"2.0\",\"id\":\"c\",\"result\":{\"content\":[],\"resultType\":\"complete\",\"resultType\":\"input_required\"}}"}}]}"#;
    for (label, raw) in [("structured", structured), ("json string", as_string)] {
        let err = parse_mcp_transcript(raw, McpInputFormat::HttpSse)
            .expect_err(&format!("must refuse a duplicate in {label} SSE data"));
        assert!(
            format!("{err:?}").contains("duplicate member"),
            "unexpected message for {label}: {err:?}"
        );
    }
}

// --- Correlation runs in source order ------------------------------------------------------------

/// The global map was last-wins, so a response could inherit the era of a *later* request. Here the
/// first request is contradicted, its response has no `resultType`, and a second request reuses the
/// id with a clean era. The response must keep the contradiction of the call it answered.
#[test]
fn a_response_takes_the_era_of_the_request_that_preceded_it() {
    let conflicted = json!({"jsonrpc": "2.0", "id": "reused", "method": "notifications/x",
                            "params": {"_meta": meta(json!(V2026))}});
    let response = json!({"jsonrpc": "2.0", "id": "reused", "result": {"content": []}});
    let clean = json!({"jsonrpc": "2.0", "id": "reused", "method": "notifications/x",
                       "params": {"_meta": meta(json!(V2025))}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "request": conflicted},
            {"timestamp_ms": 1001, "response": response},
            {"timestamp_ms": 1002, "request": clean}
        ]
    });
    let events = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    assert_eq!(
        events[1].context.era,
        EraResolution::Conflicting {
            header: V2025.into(),
            body: V2026.into()
        },
        "the response must not inherit a later request's era"
    );
}

/// A response arriving before any request keeps the era it resolved on its own.
#[test]
fn a_response_before_any_request_stays_self_resolved() {
    let response = json!({"jsonrpc": "2.0", "id": "early", "result": {"content": []}});
    let request = json!({"jsonrpc": "2.0", "id": "early", "method": "notifications/x",
                         "params": {"_meta": meta(json!(V2026))}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "response": response},
            {"timestamp_ms": 1001, "request": request}
        ]
    });
    let events = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    assert_eq!(events[0].context.era, EraResolution::Known(V2025.into()));
}

/// Two requests outstanding on one id makes the correlation ambiguous, and choosing either is a
/// silent choice between two calls. Refused on the same basis as a duplicate member.
#[test]
fn two_outstanding_requests_on_one_id_are_refused() {
    let first = json!({"jsonrpc": "2.0", "id": "dup", "method": "notifications/x", "params": {}});
    let second = json!({"jsonrpc": "2.0", "id": "dup", "method": "notifications/x", "params": {}});
    let doc = json!({
        "transport": "streamable-http",
        "entries": [
            {"timestamp_ms": 1000, "request": first},
            {"timestamp_ms": 1001, "request": second}
        ]
    });
    let err = parse_mcp_transcript(&doc.to_string(), McpInputFormat::StreamableHttp)
        .expect_err("must refuse two outstanding requests on one id");
    assert!(
        format!("{err:?}").contains("outstanding"),
        "unexpected message: {err:?}"
    );
}

/// Reusing an id *after* its response has been seen is legal, so the refusal must not become a ban
/// on reuse.
#[test]
fn sequential_id_reuse_after_a_response_is_allowed() {
    let mk_req = || {
        json!({"jsonrpc": "2.0", "id": "seq", "method": "notifications/x",
                           "params": {}})
    };
    let mk_resp = || json!({"jsonrpc": "2.0", "id": "seq", "result": {"content": []}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "request": mk_req()},
            {"timestamp_ms": 1001, "response": mk_resp()},
            {"timestamp_ms": 1002, "request": mk_req()},
            {"timestamp_ms": 1003, "response": mk_resp()}
        ]
    });
    assert!(parse_mcp_transcript(&doc.to_string(), McpInputFormat::StreamableHttp).is_ok());
}

/// An error response is a response. It deliberately has no result observation, since the
/// `resultType` requirement is about `result`, so keying correlation off that observation made it
/// invisible: it inherited nothing and consumed nothing.
#[test]
fn an_error_response_inherits_the_era_of_its_call() {
    let request = json!({"jsonrpc": "2.0", "id": "err", "method": "notifications/x",
                         "params": {"_meta": meta(json!(V2026))}});
    let error = json!({"jsonrpc": "2.0", "id": "err",
                       "error": {"code": -32000, "message": "boom"}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "request": request},
            {"timestamp_ms": 1001, "response": error}
        ]
    });
    let events = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
    assert_eq!(
        events[1].context.era,
        EraResolution::Conflicting {
            header: V2025.into(),
            body: V2026.into()
        },
        "an error response must inherit its call's era"
    );
    assert_eq!(
        events[1].context.result_observation, None,
        "and still reports no result observation, since it carries no result"
    );
}

/// Consuming the outstanding id is the other half. Without it the id stayed outstanding and a legal
/// sequential reuse tripped the two-outstanding refusal.
#[test]
fn an_error_response_frees_the_id_for_sequential_reuse() {
    let mk_req = || {
        json!({"jsonrpc": "2.0", "id": "err", "method": "notifications/x",
                           "params": {}})
    };
    let error = json!({"jsonrpc": "2.0", "id": "err",
                       "error": {"code": -32000, "message": "boom"}});
    let doc = json!({
        "transport": "streamable-http",
        "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
        "entries": [
            {"timestamp_ms": 1000, "request": mk_req()},
            {"timestamp_ms": 1001, "response": error},
            {"timestamp_ms": 1002, "request": mk_req()}
        ]
    });
    assert!(
        parse_mcp_transcript(&doc.to_string(), McpInputFormat::StreamableHttp).is_ok(),
        "an error response must consume the outstanding id"
    );
}

// --- A response is exactly one of result or error ------------------------------------------------

/// Both at once is protocol-invalid, and it licensed completion: `observe_result` reads `result`,
/// sees `complete`, and a 2026 era concludes `Terminal`. The shape has to be refused before any
/// axis reads it.
#[test]
fn a_response_carrying_both_result_and_error_is_refused() {
    let both = json!({"jsonrpc": "2.0", "id": "c",
                      "result": {"content": [], "resultType": "complete"},
                      "error": {"code": -32000, "message": "boom"}});
    let input = framed(Some(headers(json!(V2026))), None, both);
    let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp)
        .expect_err("must refuse both result and error");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("exactly one of result or error"),
        "unexpected message: {rendered}"
    );
    assert!(
        !rendered.contains("complete") && !rendered.contains("boom"),
        "refusal echoed an input value: {rendered}"
    );
}

/// Neither, with an id, was correlated as a response: it consumed the outstanding id and then let a
/// sequential reuse through. The transcript-wide gate that once shadowed this is gone, so the
/// outstanding map is what these observe.
#[test]
fn a_no_method_object_with_neither_result_nor_error_is_refused() {
    let request = json!({"jsonrpc": "2.0", "id": "n", "method": "notifications/x", "params": {}});
    let neither = json!({"jsonrpc": "2.0", "id": "n"});
    let doc = json!({
        "transport": "streamable-http",
        "entries": [
            {"timestamp_ms": 1000, "request": request},
            {"timestamp_ms": 1001, "response": neither}
        ]
    });
    let err = parse_mcp_transcript(&doc.to_string(), McpInputFormat::StreamableHttp)
        .expect_err("must refuse a response that is neither");
    assert!(
        format!("{err:?}").contains("exactly one of result or error"),
        "unexpected message: {err:?}"
    );
}

/// The controls. A valid success response and a valid error response both still correlate, so the
/// rule refuses the two invalid shapes and nothing else.
#[test]
fn valid_success_and_error_responses_still_correlate() {
    for response in [
        json!({"jsonrpc": "2.0", "id": "ok", "result": {"content": [], "resultType": "complete"}}),
        json!({"jsonrpc": "2.0", "id": "ok", "error": {"code": -1, "message": "x"}}),
    ] {
        let request = json!({"jsonrpc": "2.0", "id": "ok", "method": "notifications/x",
                             "params": {"_meta": meta(json!(V2026))}});
        let doc = json!({
            "transport": "streamable-http",
            "transport_context": {"headers": {"MCP-Protocol-Version": V2025}},
            "entries": [
                {"timestamp_ms": 1000, "request": request},
                {"timestamp_ms": 1001, "response": response.clone()}
            ]
        });
        let events = detailed(&doc.to_string(), McpInputFormat::StreamableHttp);
        assert_eq!(
            events[1].context.era,
            EraResolution::Conflicting {
                header: V2025.into(),
                body: V2026.into()
            },
            "must still inherit for {response}"
        );
    }
}

// --- Correlation keys preserve the id's JSON type ------------------------------------------------

/// Per-entry headers, so each message resolves its own era and the one a response ends up with says
/// which call it was paired to.
fn two_entry_doc(first: (Value, &str), second: (Value, &str)) -> String {
    let entry = |m: Value, v: &str, ts: u64| {
        let key = if m.get("method").is_some() {
            "request"
        } else {
            "response"
        };
        json!({"timestamp_ms": ts, "transport_context": headers(json!(v)), key: m})
    };
    json!({
        "transport": "streamable-http",
        "entries": [entry(first.0, first.1, 1000), entry(second.0, second.1, 1001)]
    })
    .to_string()
}

/// `McpEvent::jsonrpc_id` renders JSON number `1` and JSON string `"1"` as the same `String`, and
/// correlation once keyed on that rendering, so it paired them. The response consumed a call it did
/// not answer and took its era, and a missing `resultType` under that borrowed legacy era could read
/// `Terminal`. The helper that produced the rendering has since been replaced; the public field it
/// fed has not, which is why the key is derived from the raw JSON instead.
#[test]
fn a_numeric_id_does_not_correlate_with_a_string_id() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "notifications/x", "params": {}});
    let response = json!({"jsonrpc": "2.0", "id": "1", "result": {"content": []}});
    let events = detailed(
        &two_entry_doc((request, V2025), (response, V2026)),
        McpInputFormat::StreamableHttp,
    );
    assert_eq!(
        events[1].context.era,
        EraResolution::Known(V2026.into()),
        "the response must stay orphaned and keep its own era"
    );
    let observed = events[1]
        .context
        .result_observation
        .as_ref()
        .expect("a response reports a result");
    assert_eq!(
        conclude(&events[1].context.era, observed, None),
        ResultConclusion::Invalid(InvalidReason::MissingResultType)
    );
}

/// The other direction, so the rule is symmetric rather than an accident of which side renders.
#[test]
fn a_string_id_does_not_correlate_with_a_numeric_id() {
    let request = json!({"jsonrpc": "2.0", "id": "1", "method": "notifications/x", "params": {}});
    let response = json!({"jsonrpc": "2.0", "id": 1, "result": {"content": []}});
    let events = detailed(
        &two_entry_doc((request, V2025), (response, V2026)),
        McpInputFormat::StreamableHttp,
    );
    assert_eq!(events[1].context.era, EraResolution::Known(V2026.into()));
}

/// The positive controls. Same type and same value still correlate, so type-preservation did not
/// turn correlation off.
#[test]
fn same_typed_ids_still_correlate() {
    for id in [json!(1), json!("1")] {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": "notifications/x",
                             "params": {}});
        let response = json!({"jsonrpc": "2.0", "id": id, "result": {"content": []}});
        let events = detailed(
            &two_entry_doc((request, V2025), (response, V2026)),
            McpInputFormat::StreamableHttp,
        );
        assert_eq!(
            events[1].context.era,
            EraResolution::Known(V2025.into()),
            "id {id} must correlate to its own call"
        );
    }
}

/// Outstanding tracking is per typed key. A numeric `1` and a string `"1"` are different calls and
/// may both be in flight; two numeric `1`s are the ambiguous case the refusal exists for.
#[test]
fn outstanding_ids_are_tracked_per_typed_key() {
    let numeric = json!({"jsonrpc": "2.0", "id": 1, "method": "notifications/x", "params": {}});
    let stringy = json!({"jsonrpc": "2.0", "id": "1", "method": "notifications/x", "params": {}});
    assert!(
        parse_mcp_transcript(
            &two_entry_doc((numeric.clone(), V2025), (stringy, V2025)),
            McpInputFormat::StreamableHttp
        )
        .is_ok(),
        "a numeric and a string id are different calls"
    );
    let err = parse_mcp_transcript(
        &two_entry_doc((numeric.clone(), V2025), (numeric, V2025)),
        McpInputFormat::StreamableHttp,
    )
    .expect_err("two numeric 1s are one id twice");
    let rendered = format!("{err:?}");
    assert!(rendered.contains("outstanding"), "unexpected: {rendered}");
    assert!(
        !rendered.contains("notifications/x"),
        "refusal echoed input: {rendered}"
    );
}

// --- RequestId shape: classify first, then accept the id its kind requires ----------------------

/// The pinned schema says `RequestId = string | number`, so a fractional id is valid and only a null
/// is not. An earlier head refused `1.0` here on an integer-only reading taken from prose rather than
/// from the schema the ledger names as the source of truth.
#[test]
fn a_null_request_id_is_refused() {
    let message = json!({"jsonrpc": "2.0", "id": null, "method": "tools/call",
                         "params": {"name": "Calculator", "arguments": {}}});
    let input = framed(Some(headers(json!(V2026))), None, message);
    let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp)
        .expect_err("must refuse a null request id");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("must be a string or a number"),
        "unexpected: {rendered}"
    );
    assert!(
        !rendered.contains("Calculator"),
        "refusal echoed input: {rendered}"
    );
}

/// A request with no `id` member at all is a notification, which is a different shape and not a
/// refusal. The absent case is only a fault for something that classified as a request.
#[test]
fn a_notification_may_have_no_id() {
    let message = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}});
    let input = framed(Some(headers(json!(V2026))), None, message);
    assert!(parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).is_ok());
}

/// A success response answers a call, so it must name which one. A fractional id names one, so only
/// an absent or null id is a fault here.
#[test]
fn a_success_response_must_carry_an_acceptable_id() {
    for bad in [Some(json!(null)), None] {
        let mut message = json!({"jsonrpc": "2.0", "result": {"content": [],
                                                              "resultType": "complete"}});
        if let Some(id) = &bad {
            message["id"] = id.clone();
        }
        let input = framed(Some(headers(json!(V2026))), None, message);
        assert!(
            parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).is_err(),
            "must refuse a success response with id {bad:?}"
        );
    }
}

/// An error response is how a peer reports a request it could not even parse, so it may have no
/// usable id. It stays ingestible and simply correlates with nothing.
#[test]
fn an_invalid_request_error_response_may_have_no_usable_id() {
    for id in [Some(json!(null)), None] {
        let mut message = json!({"jsonrpc": "2.0",
                                 "error": {"code": -32600, "message": "Invalid Request"}});
        if let Some(id) = &id {
            message["id"] = id.clone();
        }
        let input = framed(Some(headers(json!(V2025))), None, message);
        let events = detailed(&input, McpInputFormat::StreamableHttp);
        assert_eq!(events.len(), 1, "must remain ingestible for id {id:?}");
        assert_eq!(
            events[0].context.era,
            EraResolution::Known(V2025.into()),
            "and stay uncorrelated, keeping its own era"
        );
    }
}

/// Negative and large integers are integers.
#[test]
fn negative_and_large_integers_are_accepted_ids() {
    for id in [json!(-1), json!(i64::MIN), json!(u64::MAX)] {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": "notifications/x",
                             "params": {}});
        let response = json!({"jsonrpc": "2.0", "id": id, "result": {"content": []}});
        let events = detailed(
            &two_entry_doc((request, V2025), (response, V2026)),
            McpInputFormat::StreamableHttp,
        );
        assert_eq!(
            events[1].context.era,
            EraResolution::Known(V2025.into()),
            "id {id} must be accepted and correlate"
        );
    }
}

/// The end-to-end property the whole contract exists for: an id this build will not accept must
/// never let a response borrow a legacy era and read `Terminal`. A null id is the unacceptable case
/// now that the number domain is whole.
#[test]
fn an_unacceptable_response_id_can_never_license_terminal() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "notifications/x", "params": {}});
    let response = json!({"jsonrpc": "2.0", "id": null, "result": {"content": []}});
    let doc = two_entry_doc((request, V2025), (response, V2025));
    assert!(
        parse_mcp_transcript(&doc, McpInputFormat::StreamableHttp).is_err(),
        "a null response id is refused rather than silently correlated"
    );
}

// --- Request-only methods cannot become notifications by omitting an id -------------------------

/// `tools/call` is `CallToolRequest`. Treating every method-bearing object without an `id` as a
/// notification let a request-only method shed the id requirement, and with it the required 2026
/// `RequestParams._meta`, since notification metadata is optional. End to end under 2026 with no
/// `_meta` at all.
#[test]
fn a_request_only_method_without_an_id_is_refused() {
    for method in ["tools/call", "tools/list"] {
        let message = json!({"jsonrpc": "2.0", "method": method,
                             "params": {"name": "Calculator", "arguments": {}}});
        let input = framed(Some(headers(json!(V2026))), None, message);
        let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp)
            .expect_err(&format!("{method} without an id must be refused"));
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("must be a string or a number"),
            "unexpected for {method}: {rendered}"
        );
        assert!(
            !rendered.contains("Calculator"),
            "refusal echoed input: {rendered}"
        );
    }
}

/// The positive control that keeps this from becoming a ban on notifications. A real notification,
/// and an extension method that is not one of the known request-only names, both stay ingestible
/// without an id.
#[test]
fn real_and_extension_notifications_still_need_no_id() {
    for method in ["notifications/progress", "x-vendor/telemetry"] {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": {}});
        let input = framed(Some(headers(json!(V2026))), None, message);
        assert!(
            parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).is_ok(),
            "{method} must remain ingestible without an id"
        );
    }
}

/// The positive half of the security claim. Without it an implementation that refuses every id
/// satisfies "an unacceptable id can never license Terminal" trivially.
#[test]
fn an_acceptable_integer_id_does_license_terminal_under_a_legacy_era() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "notifications/x", "params": {}});
    let response = json!({"jsonrpc": "2.0", "id": 1, "result": {"content": []}});
    let events = detailed(
        &two_entry_doc((request, V2025), (response, V2025)),
        McpInputFormat::StreamableHttp,
    );
    let observed = events[1]
        .context
        .result_observation
        .as_ref()
        .expect("a response reports a result");
    assert_eq!(observed, &ResultObservation::Missing);
    assert_eq!(
        conclude(&events[1].context.era, observed, None),
        ResultConclusion::Terminal,
        "a legacy era with an absent resultType is Terminal, which is the state the refusal protects"
    );
}

// --- RequestId is string or number, per the pinned schema ---------------------------------------

// --- Every known request method, not just the two with payloads ---------------------------------

// --- The duplicate boundary reaches the derived structs too -------------------------------------

/// `UniqueValue` covers the free-form slots, but the transcript and entry themselves are derived
/// structs, and serde's derive ignores unknown keys entirely — so a duplicate unknown member at
/// either level passed while the claim said every duplicate is refused.
#[test]
fn duplicate_members_on_the_transcript_wrapper_are_refused() {
    let raw = r#"{"transport":"streamable-http","x":1,"x":2,"entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#;
    let err = parse_mcp_transcript(raw, McpInputFormat::StreamableHttp)
        .expect_err("a duplicate on the transcript wrapper is refused");
    assert!(format!("{err:?}").contains("duplicate member"), "{err:?}");
}

#[test]
fn duplicate_members_on_an_entry_are_refused() {
    let raw = r#"{"transport":"streamable-http","entries":[{"y":1,"y":2,"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#;
    let err = parse_mcp_transcript(raw, McpInputFormat::StreamableHttp)
        .expect_err("a duplicate on an entry is refused");
    assert!(format!("{err:?}").contains("duplicate member"), "{err:?}");
}

// --- Numeric ids correlate on value, not on spelling --------------------------------------------

fn raw_pair(req_id: &str, req_ver: &str, resp_id: &str, resp_ver: &str) -> String {
    format!(
        r#"{{"transport":"streamable-http","entries":[
        {{"transport_context":{{"headers":{{"MCP-Protocol-Version":"{req_ver}"}}}},
         "request":{{"jsonrpc":"2.0","id":{req_id},"method":"notifications/x","params":{{}}}}}},
        {{"transport_context":{{"headers":{{"MCP-Protocol-Version":"{resp_ver}"}}}},
         "response":{{"jsonrpc":"2.0","id":{resp_id},"result":{{"content":[]}}}}}}]}}"#
    )
}

/// Only ids this build can key on without loss correlate. A string keys on itself; a number keys on
/// its exact text when it is an `i64` or `u64`. Every other JSON number stays ingestible and does not
/// correlate, because serde has already put it through `f64` and two different ids can land on one
/// value. A false pairing hands a response the era of a call it did not answer; declining to key
/// leaves it with its own envelope's era, which is incomplete and true.
#[test]
fn exactly_representable_integer_ids_correlate() {
    for id in ["1", "0", "-1", "9007199254740993"] {
        let events = detailed(
            &raw_pair(id, V2025, id, V2026),
            McpInputFormat::StreamableHttp,
        );
        assert_eq!(
            events[1].context.era,
            EraResolution::Known(V2025.into()),
            "id {id} must correlate to its own call"
        );
    }
}

/// A number that cannot be keyed without loss keeps its own era rather than borrowing one.
#[test]
fn a_non_representable_number_id_does_not_correlate() {
    for id in ["1.0", "1e0", "1.5", "-0.0"] {
        let events = detailed(
            &raw_pair(id, V2025, id, V2026),
            McpInputFormat::StreamableHttp,
        );
        assert_eq!(
            events[1].context.era,
            EraResolution::Known(V2026.into()),
            "id {id} must not correlate, and keeps its own envelope era"
        );
    }
}

/// The two decimal spellings either side of 2^53 both parse to the same double, so keying on them
/// would put two different ids on one call. Neither correlates, so neither can.
#[test]
fn decimal_ids_beyond_two_to_the_53_never_land_on_one_call() {
    let events = detailed(
        &raw_pair("9007199254740992.0", V2025, "9007199254740993.0", V2026),
        McpInputFormat::StreamableHttp,
    );
    assert_eq!(events[1].context.era, EraResolution::Known(V2026.into()));
}

/// A string is not a number however it is spelled.
#[test]
fn a_string_id_never_correlates_with_a_numeric_one() {
    let events = detailed(
        &raw_pair("1", V2025, "\"1\"", V2026),
        McpInputFormat::StreamableHttp,
    );
    assert_eq!(events[1].context.era, EraResolution::Known(V2026.into()));
}

/// The two integers either side of 2^53 are different ids. An `f64` key collapses them, which would
/// let one call answer another's response.
#[test]
fn integers_beyond_two_to_the_53_never_collide() {
    let events = detailed(
        &raw_pair("9007199254740992", V2025, "9007199254740993", V2026),
        McpInputFormat::StreamableHttp,
    );
    assert_eq!(
        events[1].context.era,
        EraResolution::Known(V2026.into()),
        "9007199254740992 and ...93 are different ids"
    );
}

// --- Lifetime is owned by the typed outstanding map, on real request payloads ------------------

fn tools_call(id: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"C","arguments":{{}}}}}}"#
    )
}

fn jsonrpc_lines(lines: &[String]) -> String {
    lines.join("\n")
}

/// A number `1` and a string `"1"` are different ids, so both may be in flight. The old global gate
/// keyed on the public `String` rendering and refused this.
#[test]
fn a_numeric_and_a_string_tools_call_id_may_both_be_outstanding() {
    let input = jsonrpc_lines(&[tools_call("1"), tools_call("\"1\"")]);
    assert!(
        parse_mcp_transcript(&input, McpInputFormat::JsonRpc).is_ok(),
        "number 1 and string \"1\" are different calls"
    );
}

/// Reuse after the response is legal. The old gate refused any reuse anywhere in the transcript.
#[test]
fn a_tools_call_id_may_be_reused_after_its_response() {
    let response = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[]}}"#.to_string();
    let input = jsonrpc_lines(&[tools_call("1"), response, tools_call("1")]);
    assert!(
        parse_mcp_transcript(&input, McpInputFormat::JsonRpc).is_ok(),
        "an id is free once its call has been answered"
    );
}

/// And the case the refusal exists for still refuses, value-free.
#[test]
fn two_outstanding_tools_calls_on_one_id_are_refused() {
    let input = jsonrpc_lines(&[tools_call("1"), tools_call("1")]);
    let err = parse_mcp_transcript(&input, McpInputFormat::JsonRpc)
        .expect_err("one id, two calls in flight");
    let rendered = format!("{err:?}");
    assert!(rendered.contains("outstanding"), "unexpected: {rendered}");
    assert!(
        !rendered.contains("\"C\""),
        "refusal echoed input: {rendered}"
    );
}

// --- The duplicate claim must reach every subtree ------------------------------------------------

#[test]
fn a_duplicate_inside_an_unknown_transcript_subtree_is_refused() {
    let raw = r#"{"transport":"streamable-http","junk":{"a":1,"a":2},"entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#;
    assert!(
        format!(
            "{:?}",
            parse_mcp_transcript(raw, McpInputFormat::StreamableHttp).unwrap_err()
        )
        .contains("duplicate member"),
        "a duplicate inside an ignored subtree must still be refused"
    );
}

#[test]
fn a_duplicate_inside_an_unknown_entry_subtree_is_refused() {
    let raw = r#"{"transport":"streamable-http","entries":[{"junk":{"b":1,"b":2},"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}}}]}"#;
    assert!(
        format!(
            "{:?}",
            parse_mcp_transcript(raw, McpInputFormat::StreamableHttp).unwrap_err()
        )
        .contains("duplicate member"),
        "an ignored entry subtree must still be duplicate-aware"
    );
}

#[test]
fn duplicate_members_on_the_sse_envelope_are_refused() {
    for raw in [
        r#"{"transport":"http-sse","entries":[{"sse":{"event":"message","event":"other","data":{}}}]}"#,
        r#"{"transport":"http-sse","entries":[{"sse":{"event":"message","junk":{"c":1,"c":2},"data":{}}}]}"#,
    ] {
        assert!(
            format!(
                "{:?}",
                parse_mcp_transcript(raw, McpInputFormat::HttpSse).unwrap_err()
            )
            .contains("duplicate member"),
            "SSE envelope must be duplicate-aware: {raw}"
        );
    }
}

/// `sse` written as an explicit null is a slot that arrived and failed, like `headers` and
/// `transport_context`. Folding it to absent lets it disappear silently beside a valid request.
#[test]
fn an_explicit_null_sse_slot_is_malformed_not_absent() {
    for raw in [
        r#"{"transport":"http-sse","entries":[{"sse":null}]}"#,
        r#"{"transport":"http-sse","entries":[{"request":{"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"C","arguments":{}}},"sse":null}]}"#,
    ] {
        let err = parse_mcp_transcript(raw, McpInputFormat::HttpSse).expect_err(&format!(
            "an explicit null sse slot must not fold to absent: {raw}"
        ));
        // Asserting only `is_err()` proved nothing: mutation showed that removing the null handling
        // still refuses, because deserializing `null` into the envelope fails on its own. The reason
        // is what distinguishes a slot that arrived and failed from a shape that would not parse.
        assert!(
            format!("{err:?}").contains("sse slot is present and null"),
            "refused for the wrong reason: {err:?}"
        );
    }
}

/// The whole set, not a sample. Every request method of every revision this build recognizes must be
/// refused without an id, because each one is a request and a request MUST carry one. A sample would
/// leave the untested members free to regress silently, which is how the set came to hold two names.
///
/// Collected from the `Request` interfaces of
/// `schema/{2024-11-05,2025-03-26,2025-06-18,2025-11-25,2026-07-28}` at spec commit `5f5440bb`.
#[test]
fn every_known_request_method_requires_an_id() {
    let expected = [
        "completion/complete",
        "elicitation/create",
        "initialize",
        "logging/setLevel",
        "ping",
        "prompts/get",
        "prompts/list",
        "resources/list",
        "resources/read",
        "resources/subscribe",
        "resources/templates/list",
        "resources/unsubscribe",
        "roots/list",
        "sampling/createMessage",
        "server/discover",
        "subscriptions/listen",
        "tasks/cancel",
        "tasks/get",
        "tasks/list",
        "tasks/result",
        "tools/call",
        "tools/list",
    ];
    // The table and the constant must be the same set, so adding one without the other fails here
    // rather than in a transcript.
    let mut declared: Vec<&str> = crate::mcp::era::REQUEST_ONLY_METHODS.to_vec();
    let mut listed: Vec<&str> = expected.to_vec();
    declared.sort_unstable();
    listed.sort_unstable();
    assert_eq!(declared, listed, "the table and the constant have drifted");

    for method in expected {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": {}});
        let input = framed(Some(headers(json!(V2026))), None, message);
        assert!(
            parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).is_err(),
            "{method} without an id must be refused"
        );
    }
}

/// And a name in none of the five revisions stays a notification, so the set does not swallow
/// extensions.
#[test]
fn an_unknown_extension_method_stays_a_notification() {
    for method in ["x-vendor/telemetry", "experimental/whatever", "vendor.ping"] {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": {}});
        let input = framed(Some(headers(json!(V2026))), None, message);
        assert!(
            parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).is_ok(),
            "{method} must remain ingestible without an id"
        );
    }
}

/// `data` is what an SSE frame carries. The derive made it required; the manual visitor I wrote to
/// make the envelope duplicate-aware initialized from `Default` and never checked, so a frame with
/// no `data` produced zero events instead of a refusal. A malformed frame disappearing silently is
/// the failure mode this whole slice is built against.
#[test]
fn an_sse_frame_without_data_is_refused() {
    let raw = r#"{"transport":"http-sse","entries":[{"sse":{"event":"message"}}]}"#;
    let err = parse_mcp_transcript(raw, McpInputFormat::HttpSse)
        .expect_err("an SSE frame with no data must be refused");
    assert!(
        format!("{err:?}").contains("data"),
        "refused for the wrong reason: {err:?}"
    );
}

/// The fixture helper must place a message in the slot its kind belongs to. Keying on `result`
/// alone put an error-only response under `request`, which the parser then classified as a response
/// anyway because it carries no `method` — so the harness was wrong without any assertion falling
/// over. A helper that lies quietly is the kind of thing later tests build on.
#[test]
fn the_framed_helper_places_messages_by_their_kind() {
    let cases = [
        (
            "error-only response",
            json!({"jsonrpc": "2.0", "id": "c", "error": {"code": -1, "message": "x"}}),
            "response",
        ),
        (
            "success response",
            json!({"jsonrpc": "2.0", "id": "c", "result": {"content": []}}),
            "response",
        ),
        (
            "request",
            json!({"jsonrpc": "2.0", "id": "c", "method": "tools/call",
                   "params": {"name": "C", "arguments": {}}}),
            "request",
        ),
        (
            "notification",
            json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}),
            "request",
        ),
        // A present but non-string `method` is a malformed message shape, and the wrapper has no
        // business deciding that: it belongs in the request slot, where `classify_message` refuses
        // it. Keying the wrapper on `as_str` put exactly this fixture in the response slot, so the
        // existing malformed-method test was exercising a path its own comment did not describe.
        (
            "non-string method",
            json!({"jsonrpc": "2.0", "id": "c", "method": 7,
                   "params": {"name": "C", "arguments": {}}}),
            "request",
        ),
    ];
    for (label, message, slot) in cases {
        let doc: Value =
            serde_json::from_str(&framed(None, None, message)).expect("the helper emits JSON");
        let entry = &doc["entries"][0];
        assert!(
            entry.get(slot).is_some(),
            "{label} belongs in the {slot} slot: {entry}"
        );
    }
}
