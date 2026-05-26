use super::*;
#[test]
fn contract_streamable_http_json_response_normalizes_requests_and_responses() {
    let input = json!({
        "transport": "streamable-http",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2025-06-18"
            }
        },
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "result": { "sum": 3 }
                }
            }
        ]
    })
    .to_string();

    let events = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].payload,
        McpPayload::ToolCallRequest { ref name, .. } if name == "Calculator"
    ));
    assert!(matches!(
        events[1].payload,
        McpPayload::ToolCallResponse {
            ref result,
            is_error: false,
            ..
        } if result == &json!({ "sum": 3 })
    ));
}

#[test]
fn contract_streamable_http_sse_response_parses_message_payload() {
    let input = json!({
        "transport": "streamable-http",
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "sse": {
                    "event": "message",
                    "id": "evt-1",
                    "data": {
                        "jsonrpc": "2.0",
                        "id": "call-1",
                        "result": { "sum": 3 }
                    }
                }
            }
        ]
    })
    .to_string();

    let events = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1].payload,
        McpPayload::ToolCallResponse {
            ref result,
            is_error: false,
            ..
        } if result == &json!({ "sum": 3 })
    ));
}

#[test]
fn contract_streamable_http_get_sse_notification_stays_transport_compatible() {
    let input = json!({
        "transport": "streamable-http",
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "result": { "sum": 3 }
                }
            },
            {
                "timestamp_ms": 1002,
                "sse": {
                    "event": "message",
                    "id": "evt-2",
                    "data": {
                        "jsonrpc": "2.0",
                        "method": "notifications/progress",
                        "params": {
                            "progress": 50
                        }
                    }
                }
            }
        ]
    })
    .to_string();

    let events = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap();
    assert_eq!(events.len(), 3);
    assert!(matches!(events[2].payload, McpPayload::Other { .. }));
}

#[test]
fn contract_http_sse_endpoint_event_is_ignored_for_tool_semantics() {
    let input = json!({
        "transport": "http-sse",
        "entries": [
            {
                "timestamp_ms": 1000,
                "sse": {
                    "event": "endpoint",
                    "id": "evt-0",
                    "data": "/mcp/messages?session=abc123"
                }
            },
            {
                "timestamp_ms": 1001,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1002,
                "sse": {
                    "event": "message",
                    "id": "evt-1",
                    "data": "{\"jsonrpc\":\"2.0\",\"id\":\"call-1\",\"result\":{\"sum\":3}}"
                }
            }
        ]
    })
    .to_string();

    let events = parse_mcp_transcript(&input, McpInputFormat::HttpSse).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].payload,
        McpPayload::ToolCallRequest { ref name, .. } if name == "Calculator"
    ));
    assert!(matches!(
        events[1].payload,
        McpPayload::ToolCallResponse {
            is_error: false,
            ..
        }
    ));
}

#[test]
fn contract_transport_envelope_rejects_multiple_payload_kinds_per_entry() {
    let input = json!({
        "transport": "streamable-http",
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": { "jsonrpc": "2.0", "id": "1", "method": "tools/list" },
                "response": { "jsonrpc": "2.0", "id": "1", "result": { "tools": [] } }
            }
        ]
    })
    .to_string();

    let err = parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap_err();
    assert!(
        err.to_string()
            .contains("must contain exactly one of request, response, or sse"),
        "unexpected error: {err}"
    );
}

#[test]
fn contract_transport_context_does_not_change_semantic_equivalence() {
    let base = json!({
        "transport": "streamable-http",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2025-06-18",
                "Mcp-Session-Id": "session-a"
            }
        },
        "entries": [
            {
                "timestamp_ms": 1000,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "result": { "sum": 3 }
                }
            }
        ]
    })
    .to_string();

    let variant = json!({
        "transport": "streamable-http",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2025-03-26",
                "Mcp-Session-Id": "session-b"
            }
        },
        "entries": [
            {
                "timestamp_ms": 1000,
                "transport_context": {
                    "headers": {
                        "Last-Event-ID": "evt-10"
                    }
                },
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "method": "tools/call",
                    "params": {
                        "name": "Calculator",
                        "arguments": { "a": 1, "b": 2 }
                    }
                }
            },
            {
                "timestamp_ms": 1001,
                "transport_context": {
                    "headers": {
                        "Last-Event-ID": "evt-11"
                    }
                },
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "result": { "sum": 3 }
                }
            }
        ]
    })
    .to_string();

    let base_trace = normalize_trace(&base, McpInputFormat::StreamableHttp);
    let variant_trace = normalize_trace(&variant, McpInputFormat::StreamableHttp);
    assert_eq!(base_trace, variant_trace);
}
