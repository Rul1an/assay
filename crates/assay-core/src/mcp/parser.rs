use crate::mcp::types::*;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Parse MCP transcript file contents into normalized McpEvents.
pub fn parse_mcp_transcript(text: &str, format: McpInputFormat) -> Result<Vec<McpEvent>> {
    match format {
        McpInputFormat::JsonRpc => parse_jsonrpc_jsonl(text),
        McpInputFormat::Inspector => parse_inspector_best_effort(text),
        McpInputFormat::StreamableHttp => parse_streamable_http_transcript(text),
        McpInputFormat::HttpSse => parse_http_sse_transcript(text),
    }
}

fn parse_jsonrpc_jsonl(text: &str) -> Result<Vec<McpEvent>> {
    let mut out = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let v: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSON on line {}", lineno + 1))?;

        let event = parse_jsonrpc_message(v, (lineno + 1) as u64, None)?;
        out.push(event);
    }

    Ok(out)
}

fn parse_inspector_best_effort(text: &str) -> Result<Vec<McpEvent>> {
    let v: serde_json::Value = serde_json::from_str(text).context("invalid inspector JSON")?;

    // Handle Inspector export variations:
    // 1. Array of events
    // 2. Object with "events" array
    let arr = v
        .get("events")
        .cloned()
        .or_else(|| v.as_array().cloned().map(serde_json::Value::Array))
        .and_then(|x| x.as_array().cloned())
        .unwrap_or_default();

    let mut out = Vec::new();
    for (idx, item) in arr.into_iter().enumerate() {
        // Use array index as source_line for sorting stability
        let event = parse_jsonrpc_message(item, (idx + 1) as u64, None)?;
        out.push(event);
    }

    Ok(out)
}

fn parse_streamable_http_transcript(text: &str) -> Result<Vec<McpEvent>> {
    parse_transport_transcript(text, "streamable-http", "streamable-http transcript", false)
}

fn parse_http_sse_transcript(text: &str) -> Result<Vec<McpEvent>> {
    parse_transport_transcript(text, "http-sse", "http-sse transcript", true)
}

fn parse_transport_transcript(
    text: &str,
    expected_transport: &str,
    source_label: &str,
    allow_endpoint_event: bool,
) -> Result<Vec<McpEvent>> {
    let transcript: TransportTranscript =
        serde_json::from_str(text).with_context(|| format!("invalid {}", source_label))?;

    let actual_transport = transcript.transport.as_deref().unwrap_or("missing");
    if actual_transport != expected_transport {
        bail!(
            "{} transport must be {:?}, found {:?}",
            source_label,
            expected_transport,
            actual_transport
        );
    }

    let mut out = Vec::new();
    for (idx, entry) in transcript.entries.into_iter().enumerate() {
        let source_line = (idx + 1) as u64;
        let present = usize::from(entry.request.is_some())
            + usize::from(entry.response.is_some())
            + usize::from(entry.sse.is_some());

        if present != 1 {
            bail!(
                "{} entry {} must contain exactly one of request, response, or sse",
                source_label,
                source_line
            );
        }

        if let Some(request) = entry.request {
            out.push(parse_jsonrpc_message(
                request,
                source_line,
                entry.timestamp_ms,
            )?);
            continue;
        }

        if let Some(response) = entry.response {
            out.push(parse_jsonrpc_message(
                response,
                source_line,
                entry.timestamp_ms,
            )?);
            continue;
        }

        if let Some(sse) = entry.sse {
            if let Some(jsonrpc) = extract_jsonrpc_from_sse(&sse, allow_endpoint_event) {
                out.push(parse_jsonrpc_message(
                    jsonrpc,
                    source_line,
                    entry.timestamp_ms,
                )?);
            }
        }
    }

    Ok(out)
}

fn parse_jsonrpc_message(
    v: serde_json::Value,
    source_line: u64,
    timestamp_ms_override: Option<u64>,
) -> Result<McpEvent> {
    if !v.is_object() {
        bail!(
            "MCP event at source line {} must be a JSON object",
            source_line
        );
    }

    let ts_ms = timestamp_ms_override.or_else(|| extract_ts_ms(&v));

    // JSON-RPC ID extraction
    let id_str = v
        .get("id")
        .map(|x| x.to_string().trim_matches('"').to_string());

    // Check for JSON-RPC Request (has method)
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());

    let payload = if let Some(method) = method {
        match method.as_str() {
            "tools/list" => McpPayload::ToolsListRequest { raw: v.clone() },
            "tools/call" => {
                let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
                let name = params
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown_tool")
                    .to_string();
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                McpPayload::ToolCallRequest {
                    name,
                    arguments,
                    raw: v.clone(),
                }
            }
            // Add other standard MCP methods mapping here if needed
            _ => McpPayload::Other { raw: v.clone() },
        }
    } else {
        // Response (result or error)
        if v.get("result").is_some() {
            if looks_like_tools_list_result(&v) {
                let tools = parse_tools_list_result(&v)?;
                McpPayload::ToolsListResponse {
                    tools,
                    raw: v.clone(),
                }
            } else {
                McpPayload::ToolCallResponse {
                    result: v.get("result").cloned().unwrap_or(serde_json::Value::Null),
                    is_error: false,
                    raw: v.clone(),
                }
            }
        } else if v.get("error").is_some() {
            McpPayload::ToolCallResponse {
                result: v.get("error").cloned().unwrap_or(serde_json::Value::Null),
                is_error: true,
                raw: v.clone(),
            }
        } else {
            // Maybe it's not JSON-RPC, or it's a notification/special event
            // Check for known "Session" markers if any (ad-hoc)
            McpPayload::Other { raw: v.clone() }
        }
    };

    Ok(McpEvent {
        source_line,
        timestamp_ms: ts_ms,
        jsonrpc_id: id_str,
        payload,
    })
}

fn extract_jsonrpc_from_sse(
    sse: &TransportSseEnvelope,
    allow_endpoint_event: bool,
) -> Option<serde_json::Value> {
    let event_name = sse.event.as_deref().unwrap_or("message");
    if event_name == "endpoint" && allow_endpoint_event {
        return None;
    }

    if event_name != "message" {
        return None;
    }

    extract_jsonrpc_like_value(&sse.data)
}

fn extract_jsonrpc_like_value(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Object(map)
            if map.contains_key("method")
                || map.contains_key("result")
                || map.contains_key("error")
                || map.contains_key("jsonrpc") =>
        {
            Some(value.clone())
        }
        serde_json::Value::String(text) => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|parsed| extract_jsonrpc_like_value(&parsed)),
        _ => None,
    }
}

fn extract_ts_ms(v: &serde_json::Value) -> Option<u64> {
    // Try standard keys.
    if let Some(t) = v.get("timestamp_ms").and_then(|t| t.as_u64()) {
        return Some(t);
    }
    if let Some(t) = v.get("timestamp").and_then(|t| t.as_u64()) {
        return Some(t); // Assume ms if big integer, otherwise might be seconds?
                        // For P0, assume ms or handled by caller if not.
    }
    None
}

fn looks_like_tools_list_result(v: &serde_json::Value) -> bool {
    v.get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .is_some()
}

fn parse_tools_list_result(v: &serde_json::Value) -> Result<Vec<McpToolDef>> {
    let tools = v
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string();
        let description = tool
            .get("description")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        // Handle inputSchema (camelCase) or input_schema (snake_case)
        let input_schema = tool
            .get("inputSchema")
            .cloned()
            .or_else(|| tool.get("input_schema").cloned());
        out.push(McpToolDef {
            name,
            description,
            input_schema,
            tool_identity: None,
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct TransportTranscript {
    transport: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    transport_context: Option<serde_json::Value>,
    #[allow(dead_code)]
    #[serde(default)]
    headers: Option<serde_json::Value>,
    #[serde(default)]
    entries: Vec<TransportTranscriptEntry>,
}

#[derive(Debug, Deserialize)]
struct TransportTranscriptEntry {
    #[serde(default)]
    timestamp_ms: Option<u64>,
    #[allow(dead_code)]
    #[serde(default)]
    transport_context: Option<serde_json::Value>,
    #[allow(dead_code)]
    #[serde(default)]
    headers: Option<serde_json::Value>,
    #[serde(default)]
    request: Option<serde_json::Value>,
    #[serde(default)]
    response: Option<serde_json::Value>,
    #[serde(default)]
    sse: Option<TransportSseEnvelope>,
}

#[derive(Debug, Deserialize)]
struct TransportSseEnvelope {
    #[serde(default)]
    event: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    id: Option<String>,
    data: serde_json::Value,
}
