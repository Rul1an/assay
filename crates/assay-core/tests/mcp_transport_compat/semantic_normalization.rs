use super::*;
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
