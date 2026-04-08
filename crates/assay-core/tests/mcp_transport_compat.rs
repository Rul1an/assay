use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat, McpPayload};
use assay_core::trace::schema::TraceEvent;
use serde_json::{json, Value};

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

#[test]
fn contract_transport_families_normalize_to_same_semantics() {
    let jsonrpc = r#"
{"timestamp_ms":1000,"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"Calculator","arguments":{"a":1,"b":2}}}
{"timestamp_ms":1001,"jsonrpc":"2.0","id":"call-1","result":{"sum":3}}
{"timestamp_ms":1002,"jsonrpc":"2.0","id":"call-2","method":"tools/call","params":{"name":"Divider","arguments":{"a":6,"b":0}}}
{"timestamp_ms":1003,"jsonrpc":"2.0","id":"call-2","error":{"code":400,"message":"division by zero"}}
{"timestamp_ms":1004,"jsonrpc":"2.0","id":"ghost","result":{"ghost":true}}
"#;

    let streamable_http_json = json!({
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
            },
            {
                "timestamp_ms": 1002,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-2",
                    "method": "tools/call",
                    "params": {
                        "name": "Divider",
                        "arguments": { "a": 6, "b": 0 }
                    }
                }
            },
            {
                "timestamp_ms": 1003,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-2",
                    "error": { "code": 400, "message": "division by zero" }
                }
            },
            {
                "timestamp_ms": 1004,
                "response": {
                    "jsonrpc": "2.0",
                    "id": "ghost",
                    "result": { "ghost": true }
                }
            }
        ]
    })
    .to_string();

    let streamable_http_sse = json!({
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
                "sse": {
                    "event": "message",
                    "id": "evt-1",
                    "data": {
                        "jsonrpc": "2.0",
                        "id": "call-1",
                        "result": { "sum": 3 }
                    }
                }
            },
            {
                "timestamp_ms": 1002,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-2",
                    "method": "tools/call",
                    "params": {
                        "name": "Divider",
                        "arguments": { "a": 6, "b": 0 }
                    }
                }
            },
            {
                "timestamp_ms": 1003,
                "sse": {
                    "event": "message",
                    "id": "evt-2",
                    "data": {
                        "jsonrpc": "2.0",
                        "id": "call-2",
                        "error": { "code": 400, "message": "division by zero" }
                    }
                }
            },
            {
                "timestamp_ms": 1004,
                "sse": {
                    "event": "message",
                    "id": "evt-3",
                    "data": {
                        "jsonrpc": "2.0",
                        "id": "ghost",
                        "result": { "ghost": true }
                    }
                }
            }
        ]
    })
    .to_string();

    let http_sse = json!({
        "transport": "http-sse",
        "transport_context": {
            "headers": {
                "MCP-Protocol-Version": "2024-11-05",
                "Mcp-Session-Id": "legacy-session"
            }
        },
        "entries": [
            {
                "timestamp_ms": 999,
                "sse": {
                    "event": "endpoint",
                    "id": "evt-0",
                    "data": "/mcp/messages?session=legacy-session"
                }
            },
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
                    "data": "{\"jsonrpc\":\"2.0\",\"id\":\"call-1\",\"result\":{\"sum\":3}}"
                }
            },
            {
                "timestamp_ms": 1002,
                "request": {
                    "jsonrpc": "2.0",
                    "id": "call-2",
                    "method": "tools/call",
                    "params": {
                        "name": "Divider",
                        "arguments": { "a": 6, "b": 0 }
                    }
                }
            },
            {
                "timestamp_ms": 1003,
                "sse": {
                    "event": "message",
                    "id": "evt-2",
                    "data": "{\"jsonrpc\":\"2.0\",\"id\":\"call-2\",\"error\":{\"code\":400,\"message\":\"division by zero\"}}"
                }
            },
            {
                "timestamp_ms": 1004,
                "sse": {
                    "event": "message",
                    "id": "evt-3",
                    "data": "{\"jsonrpc\":\"2.0\",\"id\":\"ghost\",\"result\":{\"ghost\":true}}"
                }
            }
        ]
    })
    .to_string();

    let baseline = normalize_trace(jsonrpc, McpInputFormat::JsonRpc);
    assert_eq!(
        baseline,
        normalize_trace(&streamable_http_json, McpInputFormat::StreamableHttp)
    );
    assert_eq!(
        baseline,
        normalize_trace(&streamable_http_sse, McpInputFormat::StreamableHttp)
    );
    assert_eq!(
        baseline,
        normalize_trace(&http_sse, McpInputFormat::HttpSse)
    );
}

#[test]
fn contract_streamable_http_401_www_authenticate_promotes_k2_auth_discovery() {
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
                "transport_context": {
                    "status": 401,
                    "headers": {
                        "WWW-Authenticate": "Bearer resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\", scope=\"tools/call\""
                    }
                },
                "response": {
                    "jsonrpc": "2.0",
                    "id": "call-1",
                    "error": { "code": 401, "message": "unauthorized" }
                }
            }
        ]
    })
    .to_string();

    let trace = mcp_events_to_v2_trace(
        parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap(),
        "authz_discovery".into(),
        None,
        None,
    );

    let TraceEvent::EpisodeStart(start) = &trace[0] else {
        panic!("first event must be episode_start");
    };

    assert_eq!(
        start.meta["mcp"]["authorization_discovery"],
        json!({
            "visible": true,
            "source_kind": "www_authenticate",
            "resource_metadata_visible": true,
            "authorization_servers_visible": false,
            "scope_challenge_visible": true
        })
    );
}

#[test]
fn contract_www_authenticate_not_promoted_without_401_status() {
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
                "transport_context": {
                    "status": 200,
                    "headers": {
                        "WWW-Authenticate": "Bearer resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\", scope=\"tools/call\""
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

    let trace = mcp_events_to_v2_trace(
        parse_mcp_transcript(&input, McpInputFormat::StreamableHttp).unwrap(),
        "authz_discovery".into(),
        None,
        None,
    );

    let TraceEvent::EpisodeStart(start) = &trace[0] else {
        panic!("first event must be episode_start");
    };

    assert_eq!(
        start.meta["mcp"]["authorization_discovery"],
        json!({
            "visible": false,
            "source_kind": "unknown",
            "resource_metadata_visible": false,
            "authorization_servers_visible": false,
            "scope_challenge_visible": false
        })
    );
}

fn normalize_trace(input: &str, format: McpInputFormat) -> Value {
    let events = parse_mcp_transcript(input, format).expect("parse transcript");
    let trace = mcp_events_to_v2_trace(events, "transport_ep".into(), None, None);

    Value::Array(
        trace
            .iter()
            .map(|event| match event {
                TraceEvent::EpisodeStart(start) => json!({
                    "type": "episode_start",
                    "prompt": start.input["prompt"],
                }),
                TraceEvent::Step(step) => json!({
                    "type": "step",
                    "idx": step.idx,
                    "kind": step.kind,
                    "name": step.name,
                    "jsonrpc_id": step.meta.get("jsonrpc_id").cloned().unwrap_or(Value::Null),
                }),
                TraceEvent::ToolCall(call) => json!({
                    "type": "tool_call",
                    "step_id": call.step_id,
                    "tool_name": call.tool_name,
                    "args": call.args,
                    "result": call.result,
                    "error": call.error,
                }),
                TraceEvent::EpisodeEnd(end) => json!({
                    "type": "episode_end",
                    "final_output": end.final_output,
                }),
            })
            .collect(),
    )
}
