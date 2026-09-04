use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat};
use assay_core::trace::schema::TraceEvent;
use serde_json::json;

#[test]
fn test_mcp_correlation_and_prompt() {
    let input = r#"
{"jsonrpc":"2.0", "id":"req1", "method":"tools/call", "params":{"name":"Calculator", "arguments":{"a":1, "b":2}}}
{"jsonrpc":"2.0", "id":"req1", "result": 3}
"#;

    let events = parse_mcp_transcript(input, McpInputFormat::JsonRpc).unwrap();
    let trace = mcp_events_to_v2_trace(events, "test_ep".into(), None, Some("test_prompt".into()));

    // Check EpisodeStart (P0.1)
    if let TraceEvent::EpisodeStart(start) = &trace[0] {
        assert_eq!(start.input["prompt"], "test_prompt");
    } else {
        panic!("First event should be EpisodeStart");
    }

    // Check ToolCall Correlation (P0.2)
    // Expect: Step(req1) -> ToolCall(req1 merged)
    let tool_call = trace
        .iter()
        .find_map(|e| match e {
            TraceEvent::ToolCall(tc) => Some(tc),
            _ => None,
        })
        .expect("Should have one ToolCall");

    assert_eq!(tool_call.tool_name, "Calculator");
    assert_eq!(tool_call.args, json!({"a": 1, "b": 2}));
    assert_eq!(tool_call.result, Some(json!(3)));
}

#[test]
fn test_determinism_line_fallback() {
    // P0.3: No timestamps, rely on line order.
    //
    // Each request carries a unique id because `tools/list` and `tools/call` are requests and MCP
    // requires a string or integer id on one. The fixture previously omitted them, which is what let
    // a request-only method be read as a notification and shed both the id requirement and the
    // required 2026 request metadata. This test is about deterministic line fallback, not about
    // ingesting protocol-invalid requests, and the ids change nothing it asserts.
    let input = r#"
{"jsonrpc":"2.0", "id":"list-1", "method":"tools/list"}
{"jsonrpc":"2.0", "id":"call-a", "method":"tools/call", "params":{"name":"A", "arguments":{}}}
{"jsonrpc":"2.0", "id":"call-b", "method":"tools/call", "params":{"name":"B", "arguments":{}}}
"#;

    let events = parse_mcp_transcript(input, McpInputFormat::JsonRpc).unwrap();
    // Check line numbers (lines 2, 3, 4)
    assert_eq!(events[0].source_line, 2);
    assert_eq!(events[1].source_line, 3);
    assert_eq!(events[2].source_line, 4);

    let trace = mcp_events_to_v2_trace(events, "order_test".into(), None, None);

    // Check order of Step kinds/names
    let steps: Vec<String> = trace
        .iter()
        .filter_map(|e| match e {
            TraceEvent::Step(s) => s.name.clone(),
            _ => None,
        })
        .collect();

    assert_eq!(
        steps,
        vec!["tools/list".to_string(), "A".to_string(), "B".to_string()]
    );
}

#[test]
fn test_mcp_prompt_override_multibyte_over_4096_nontruncation() {
    // #2786 Slice B: Pin that an over-4096-byte multibyte MCP prompt_override
    // remains byte-identical in EpisodeStart.input.prompt.
    //
    // Note: The MCP mapper does not run Assay's truncator (`truncate_value_with_provenance`
    // or `apply_truncation`). The prompt is passed directly into `EpisodeStart.input["prompt"]`.
    // Empty or absent truncation metadata reflects mapper pass-through rather than a measured-clean
    // result, and this test makes no claim of upstream completeness.
    let input = r#"
{"jsonrpc":"2.0", "id":"req1", "method":"tools/call", "params":{"name":"Calculator", "arguments":{"a":1, "b":2}}}
{"jsonrpc":"2.0", "id":"req1", "result": 3}
"#;

    let chunk = "🦀 prompt payload with multibyte 日本語 and €uro: ";
    let repeat_count = (5000 / chunk.len()) + 1;
    let long_prompt = chunk.repeat(repeat_count);
    assert!(long_prompt.len() > 4096);
    assert!(!long_prompt.is_ascii());

    let events = parse_mcp_transcript(input, McpInputFormat::JsonRpc).unwrap();
    let trace = mcp_events_to_v2_trace(
        events,
        "test_ep_long".into(),
        None,
        Some(long_prompt.clone()),
    );

    match &trace[0] {
        TraceEvent::EpisodeStart(start) => {
            let prompt = start
                .input
                .get("prompt")
                .and_then(|v| v.as_str())
                .expect("prompt field present");
            assert_eq!(
                prompt,
                long_prompt.as_str(),
                "MCP prompt_override must remain byte-identical without truncation"
            );
            assert_eq!(prompt.len(), long_prompt.len());
        }
        _ => panic!("First event should be EpisodeStart"),
    }
}
