use crate::mcp::types::*;
use anyhow::{Context, Result};

/// Parse MCP transcript file contents into normalized McpEvents.
pub fn parse_mcp_transcript(text: &str, format: McpInputFormat) -> Result<Vec<McpEvent>> {
    match format {
        McpInputFormat::JsonRpc => parse_jsonrpc_jsonl(text),
        McpInputFormat::Inspector => parse_inspector_best_effort(text),
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

        let event = parse_single_event(v, (lineno + 1) as u64)?;
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
        let event = parse_single_event(item, (idx + 1) as u64)?;
        out.push(event);
    }

    Ok(out)
}

fn parse_single_event(v: serde_json::Value, source_line: u64) -> Result<McpEvent> {
    let ts_ms = extract_ts_ms(&v);

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
