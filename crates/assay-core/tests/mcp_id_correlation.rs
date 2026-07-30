use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat};
use assay_core::trace::schema::TraceEvent;
use serde_json::json;

#[test]
fn contract_string_id_correlates() {
    let trace = normalize_trace(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"StringId","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"req-1","result":{"ok":true}}
"#,
    );

    assert_tool_call(
        &trace,
        "StringId",
        json!({"x": 1}),
        Some(json!({"ok": true})),
        None,
    );
}

#[test]
fn contract_numeric_id_canonicalizes_and_correlates() {
    let events = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"NumericId","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":7,"result":{"ok":true}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap();

    assert_eq!(events[0].jsonrpc_id.as_deref(), Some("7"));

    let trace = mcp_events_to_v2_trace(events, "numeric".into(), None, None);
    assert_tool_call(
        &trace,
        "NumericId",
        json!({"x": 1}),
        Some(json!({"ok": true})),
        None,
    );
}

#[test]
fn contract_null_id_on_a_request_fails_hard() {
    // MCP restricts `RequestId` to a string or a number, and a request has to name the call it
    // belongs to. This transcript used to parse with both ids normalized to `None`, which meant a
    // request with no usable identity was ingested and silently correlated with nothing.
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":null,"method":"tools/call","params":{"name":"NullId","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect_err("a request with a null id is refused");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("must be a string or a number"),
        "{rendered}"
    );
    assert!(
        !rendered.contains("NullId"),
        "refusal echoed input: {rendered}"
    );
}

#[test]
fn contract_null_id_on_an_error_response_remains_ingestible() {
    // The control that keeps the rule from becoming a blanket ban on `null`. An error response is
    // how a peer reports a request it could not parse, so it may have no usable id at all. It stays
    // ingestible and correlates with nothing.
    let events = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect("an invalid-request error response is ingestible");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].jsonrpc_id, None);

    let trace = mcp_events_to_v2_trace(events, "null-id-error".into(), None, None);
    assert_no_jsonrpc_id_literal_null(&trace);
}

#[test]
fn contract_request_only_method_without_an_id_fails_hard() {
    // This transcript used to be accepted and normalized as a tool call. `tools/call` is
    // `CallToolRequest` and a request MUST carry an id, so treating a method-bearing
    // object without an `id` member as a notification let a request-only method shed the id
    // requirement — and with it the required 2026 `RequestParams._meta`, since notification metadata
    // is optional.
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","method":"tools/call","params":{"name":"MissingRequestId","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect_err("a request-only method without an id is refused");
    assert!(
        format!("{err:?}").contains("must be a string or a number"),
        "{err:?}"
    );
}

#[test]
fn contract_an_orphan_response_does_not_correlate() {
    // The property the previous test was really about, without needing a malformed request to
    // demonstrate it: a well-formed response whose call is simply not in the transcript.
    let events = parse_mcp_transcript(
        r#"
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"ghost","result":{"ok":true}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect("an orphan response is ingestible");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].jsonrpc_id.as_deref(), Some("ghost"));
}

#[test]
fn contract_missing_success_response_id_fails_hard() {
    // A success response answers a call, so it must say which one. This used to be ingested and
    // reported as `timeout/no_response` against the pending request, which reads as evidence about
    // the call rather than as a defect in the record.
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"MissingResponseId","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","result":{"ok":true}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect_err("a success response with no id is refused");
    assert!(
        format!("{err:?}").contains("must be a string or a number"),
        "{err:?}"
    );
}

#[test]
fn contract_mismatch_ids_leave_response_orphan_and_request_pending() {
    let events = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"MismatchId","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"req-2","result":{"ok":true}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap();

    let trace = mcp_events_to_v2_trace(events, "mismatch-id".into(), None, None);
    assert_tool_call(
        &trace,
        "MismatchId",
        json!({"x": 1}),
        None,
        Some("timeout/no_response"),
    );
}

#[test]
fn contract_two_outstanding_request_ids_fail_hard() {
    // Still fail-hard, and now from the authority that owns call lifetime rather than from a global
    // gate that ran before correlation. The diagnostic no longer echoes the id: it is input-chosen,
    // and naming it told a reader nothing they could act on.
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"dup-1","method":"tools/call","params":{"name":"First","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"dup-1","method":"tools/call","params":{"name":"Second","arguments":{"x":2}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap_err();

    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("outstanding"),
        "unexpected error: {rendered}"
    );
    assert!(
        !rendered.contains("dup-1"),
        "refusal echoed the id: {rendered}"
    );
}

#[test]
fn contract_a_request_id_may_be_reused_after_its_response() {
    // The old gate refused this. An id names a call in flight, and once the call is answered the id
    // is free; JSON-RPC has no rule against reuse and long transcripts rely on it.
    parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"reused","method":"tools/call","params":{"name":"First","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"reused","result":{"ok":true}}
{"timestamp_ms":1002,"jsonrpc":"2.0","id":"reused","method":"tools/call","params":{"name":"Second","arguments":{"x":2}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect("an id is free once its call has been answered");
}

#[test]
fn contract_a_numeric_and_a_string_id_are_different_calls() {
    // The old gate keyed on the public `String` rendering, which made these one id.
    parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"Numeric","arguments":{}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"1","method":"tools/call","params":{"name":"Stringy","arguments":{}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .expect("number 1 and string \"1\" are different ids");
}

#[test]
fn contract_first_response_wins_and_later_duplicate_responses_are_orphan() {
    let events = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"FirstMatchWins","arguments":{"x":1}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"req-1","result":{"ok":true}}
{"timestamp_ms":1002,"jsonrpc":"2.0","id":"req-1","result":{"ok":"late"}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap();

    let trace = mcp_events_to_v2_trace(events, "first-match-wins".into(), None, None);
    let tool_calls: Vec<_> = trace
        .iter()
        .filter_map(|event| match event {
            TraceEvent::ToolCall(call) => Some(call),
            _ => None,
        })
        .collect();

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].result, Some(json!({"ok": true})));
    assert_eq!(tool_calls[0].error, None);

    let end = trace
        .iter()
        .find_map(|event| match event {
            TraceEvent::EpisodeEnd(end) => Some(end),
            _ => None,
        })
        .expect("episode end");
    assert_eq!(end.final_output.as_deref(), Some("{\"ok\":\"late\"}"));
}

#[test]
fn contract_bool_id_true_fails_hard() {
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":true,"method":"tools/call","params":{"name":"BoolTrue","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("must not be a boolean"),
        "unexpected error: {err}"
    );
}

#[test]
fn contract_bool_id_false_fails_hard() {
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":false,"method":"tools/call","params":{"name":"BoolFalse","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("must not be a boolean"),
        "unexpected error: {err}"
    );
}

#[test]
fn contract_object_id_fails_hard() {
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":{"bad":1},"method":"tools/call","params":{"name":"ObjectId","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("must not be an object"),
        "unexpected error: {err}"
    );
}

#[test]
fn contract_array_id_fails_hard() {
    let err = parse_mcp_transcript(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":[1,2],"method":"tools/call","params":{"name":"ArrayId","arguments":{"x":1}}}
"#,
        McpInputFormat::JsonRpc,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("must not be an array"),
        "unexpected error: {err}"
    );
}

fn normalize_trace(input: &str) -> Vec<TraceEvent> {
    let events = parse_mcp_transcript(input, McpInputFormat::JsonRpc).expect("parse transcript");
    mcp_events_to_v2_trace(events, "id-correlation".into(), None, None)
}

fn assert_tool_call(
    trace: &[TraceEvent],
    tool_name: &str,
    args: serde_json::Value,
    result: Option<serde_json::Value>,
    error: Option<&str>,
) {
    let tool_call = trace
        .iter()
        .find_map(|event| match event {
            TraceEvent::ToolCall(call) if call.tool_name == tool_name => Some(call),
            _ => None,
        })
        .expect("tool call");

    assert_eq!(tool_call.args, args);
    assert_eq!(tool_call.result, result);
    assert_eq!(tool_call.error.as_deref(), error);
}

fn assert_no_jsonrpc_id_literal_null(trace: &[TraceEvent]) {
    for event in trace {
        if let TraceEvent::Step(step) = event {
            assert_ne!(
                step.meta.get("jsonrpc_id"),
                Some(&json!("null")),
                "JSON null must not normalize to literal string \"null\"",
            );
        }
    }
}

/// Pairs observed in the emitted trace, as `(tool_name, result)`.
fn tool_call_pairs(input: &str) -> Vec<(String, String)> {
    let events = parse_mcp_transcript(input, McpInputFormat::JsonRpc).expect("parse");
    mcp_events_to_v2_trace(events, "typed-ids".into(), None, None)
        .into_iter()
        .filter_map(|e| match e {
            TraceEvent::ToolCall(c) => Some((
                c.tool_name,
                c.result.map(|r| r.to_string()).unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The parser tells a JSON number `1` from a JSON string `"1"`, but the trace mapper keyed on the
/// public `String` rendering, so the second request overwrote the first and a numeric response was
/// attached to the string request.
#[test]
fn contract_typed_ids_stay_distinct_through_the_trace_mapper() {
    let pairs = tool_call_pairs(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"1","method":"tools/call","params":{"name":"Stringy","arguments":{"owner":"string"}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"Numeric","arguments":{"owner":"numeric"}}}
{"timestamp_ms":1002,"jsonrpc":"2.0","id":1,"result":{"owner":"numeric-response"}}
"#,
    );
    let numeric = pairs
        .iter()
        .find(|(name, _)| name == "Numeric")
        .expect("the numeric call");
    assert!(
        numeric.1.contains("numeric-response"),
        "the numeric response belongs to the numeric call: {pairs:?}"
    );
    let stringy = pairs
        .iter()
        .find(|(name, _)| name == "Stringy")
        .expect("the string call");
    assert!(
        !stringy.1.contains("numeric-response"),
        "the string call must not receive the numeric response: {pairs:?}"
    );
}

/// A number the correlator declines to key must not be paired through the public string rendering
/// either. Both sides render as `1.5`, so a rendering-keyed map would pair them.
#[test]
fn contract_a_non_correlatable_numeric_id_is_never_paired() {
    let pairs = tool_call_pairs(
        r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":1.5,"method":"tools/call","params":{"name":"Fractional","arguments":{"owner":"req"}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":1.5,"result":{"owner":"resp"}}
"#,
    );
    let call = pairs
        .iter()
        .find(|(name, _)| name == "Fractional")
        .expect("the fractional call");
    assert!(
        !call.1.contains("resp"),
        "an id the correlator will not key must not pair: {pairs:?}"
    );
}
