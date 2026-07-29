use crate::mcp::era::{
    fold_envelope, observe_header, observe_request_metadata, observe_result, resolve_era,
    EnvelopeObservation, McpEraContext, ParsedMcpEvent, RequestMetadata,
};
use crate::mcp::types::*;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;

/// Parse MCP transcript file contents into normalized McpEvents.
/// Public surface, unchanged. Projects the events out of the detailed parse and drops the era
/// sidecar, so downstream `McpEvent` consumers see exactly what they saw before this slice.
pub fn parse_mcp_transcript(text: &str, format: McpInputFormat) -> Result<Vec<McpEvent>> {
    Ok(parse_mcp_transcript_detailed(text, format)?
        .into_iter()
        .map(|parsed| parsed.event)
        .collect())
}

/// Internal parse that keeps what the public event shape cannot carry.
///
/// The envelope is folded per entry, from the transcript-level slots each entry starts with plus
/// its own, so a later deviant entry cannot reach back and contaminate an earlier correct one. The
/// request and result signals are per message and are read from the payload's retained raw JSON,
/// before any projection can lose them.
pub(crate) fn parse_mcp_transcript_detailed(
    text: &str,
    format: McpInputFormat,
) -> Result<Vec<ParsedMcpEvent>> {
    let events = parse_events(text, format)?;
    let framed = is_framed(format);
    let envelopes = observe_envelopes(text, format);
    Ok(events
        .into_iter()
        .map(|event| {
            let raw = payload_raw(&event.payload).cloned();
            let raw = raw.as_ref();
            // A response carries no request metadata, so it gets no observation rather than an
            // "absent" one, which would be a claim about a field that does not apply to it.
            // The same discriminant `parse_jsonrpc_message` uses: a valid string `method`. Keying
            // on the absence of `result`/`error` instead would drop the observation for a
            // malformed hybrid that carries both, which is exactly the shape worth observing.
            let request_metadata = raw
                .filter(|r| r.get("method").and_then(|m| m.as_str()).is_some())
                .map(observe_request_metadata);
            // Transport entries number from one; every other format has a single whole-input
            // observation, so index 0 serves them all.
            // Unframed formats have one whole-input observation and no entries to index, so
            // index 0 is their answer. A framed input that does not resolve to an entry is a
            // mapping the parser cannot vouch for, and a plausible-but-wrong attribution is worse
            // than an unusable one, so it fails closed rather than borrowing another entry's.
            let envelope = if framed {
                envelopes
                    .get(event.source_line.saturating_sub(1) as usize)
                    .cloned()
                    .unwrap_or(EnvelopeObservation::Malformed)
            } else {
                envelopes
                    .first()
                    .cloned()
                    .unwrap_or(EnvelopeObservation::NotApplicable)
            };
            let era = resolve_era(
                &envelope,
                request_metadata
                    .as_ref()
                    .unwrap_or(&RequestMetadata::Absent),
            );
            ParsedMcpEvent {
                event,
                context: McpEraContext {
                    envelope,
                    era,
                    request_metadata,
                    result_observation: raw.and_then(observe_result),
                },
            }
        })
        .collect())
}

fn payload_raw(payload: &McpPayload) -> Option<&serde_json::Value> {
    match payload {
        McpPayload::SessionStart { raw }
        | McpPayload::ToolsListRequest { raw }
        | McpPayload::ToolsListResponse { raw, .. }
        | McpPayload::ToolCallRequest { raw, .. }
        | McpPayload::ToolCallResponse { raw, .. }
        | McpPayload::SessionEnd { raw, .. }
        | McpPayload::Other { raw, .. } => Some(raw),
    }
}

/// Every header slot one object carries: the nested `transport_context.headers` and the direct
/// `headers` field that both the transcript and each entry also have.
fn header_slots(node: Option<&serde_json::Value>, into: &mut Vec<EnvelopeObservation>) {
    let Some(node) = node else { return };
    for headers in [
        node.get("transport_context").and_then(|c| c.get("headers")),
        node.get("headers"),
    ] {
        if let Some(o) = observe_header(headers) {
            into.push(o);
        }
    }
}

/// One envelope observation per entry, so a later deviant entry cannot retroactively contaminate
/// an earlier correct one. The transcript-level slots are the default each entry starts from.
/// Which formats carry transport framing at all. `Inspector` reads an events array straight into
/// `parse_jsonrpc_message` and never reaches `parse_transport_transcript`, so it has no envelope.
fn is_framed(format: McpInputFormat) -> bool {
    matches!(
        format,
        McpInputFormat::StreamableHttp | McpInputFormat::HttpSse
    )
}

fn observe_envelopes(text: &str, format: McpInputFormat) -> Vec<EnvelopeObservation> {
    let framed = is_framed(format);
    if !framed {
        return vec![EnvelopeObservation::NotApplicable];
    }
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![EnvelopeObservation::Absent];
    };
    let mut transcript_slots = Vec::new();
    header_slots(Some(&doc), &mut transcript_slots);
    let entries = doc
        .get("entries")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    if entries.is_empty() {
        return vec![fold_envelope(transcript_slots, framed)];
    }
    entries
        .iter()
        .map(|entry| {
            let mut slots = transcript_slots.clone();
            header_slots(Some(entry), &mut slots);
            fold_envelope(slots, framed)
        })
        .collect()
}

fn parse_events(text: &str, format: McpInputFormat) -> Result<Vec<McpEvent>> {
    let events = match format {
        McpInputFormat::JsonRpc => parse_jsonrpc_jsonl(text),
        McpInputFormat::Inspector => parse_inspector_best_effort(text),
        McpInputFormat::StreamableHttp => parse_streamable_http_transcript(text),
        McpInputFormat::HttpSse => parse_http_sse_transcript(text),
    }?;
    validate_mcp_events(&events)?;
    Ok(events)
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

        let event = parse_jsonrpc_message(
            v,
            (lineno + 1) as u64,
            None,
            McpAuthorizationDiscovery::default(),
        )?;
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
        let event = parse_jsonrpc_message(
            item,
            (idx + 1) as u64,
            None,
            McpAuthorizationDiscovery::default(),
        )?;
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
                McpAuthorizationDiscovery::default(),
            )?);
            continue;
        }

        let auth_discovery = parse_transport_auth_discovery(&entry);

        if let Some(response) = entry.response {
            out.push(parse_jsonrpc_message(
                response,
                source_line,
                entry.timestamp_ms,
                auth_discovery,
            )?);
            continue;
        }

        if let Some(sse) = entry.sse {
            if let Some(jsonrpc) = extract_jsonrpc_from_sse(&sse, allow_endpoint_event) {
                out.push(parse_jsonrpc_message(
                    jsonrpc,
                    source_line,
                    entry.timestamp_ms,
                    McpAuthorizationDiscovery::default(),
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
    auth_discovery: McpAuthorizationDiscovery,
) -> Result<McpEvent> {
    if !v.is_object() {
        bail!(
            "MCP event at source line {} must be a JSON object",
            source_line
        );
    }

    let ts_ms = timestamp_ms_override.or_else(|| extract_ts_ms(&v));

    // JSON-RPC ID extraction
    let id_str = normalize_jsonrpc_id(v.get("id"), source_line)?;

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
        auth_discovery,
        payload,
    })
}

fn parse_transport_auth_discovery(entry: &TransportTranscriptEntry) -> McpAuthorizationDiscovery {
    let Some(status) = extract_http_status(entry) else {
        return McpAuthorizationDiscovery::default();
    };

    if status != 401 {
        return McpAuthorizationDiscovery::default();
    }

    let header_value = entry
        .transport_context
        .as_ref()
        .and_then(|value| find_header_case_insensitive(value, "www-authenticate"))
        .or_else(|| {
            entry
                .headers
                .as_ref()
                .and_then(|value| find_header_case_insensitive(value, "www-authenticate"))
        });

    let Some(www_authenticate) = header_value else {
        return McpAuthorizationDiscovery::default();
    };

    let resource_metadata_visible = auth_param_visible(&www_authenticate, "resource_metadata");
    let scope_challenge_visible = auth_param_visible(&www_authenticate, "scope");

    if !resource_metadata_visible && !scope_challenge_visible {
        return McpAuthorizationDiscovery::default();
    }

    McpAuthorizationDiscovery {
        visible: true,
        source_kind: McpAuthorizationDiscoverySourceKind::WwwAuthenticate,
        resource_metadata_visible,
        authorization_servers_visible: false,
        scope_challenge_visible,
    }
}

fn extract_http_status(entry: &TransportTranscriptEntry) -> Option<u16> {
    entry
        .transport_context
        .as_ref()
        .and_then(extract_http_status_from_value)
        .or_else(|| {
            entry
                .headers
                .as_ref()
                .and_then(extract_http_status_from_value)
        })
}

fn extract_http_status_from_value(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["status", "status_code", "http_status"] {
                if let Some(status) = map.get(key).and_then(json_value_to_u16) {
                    return Some(status);
                }
            }

            map.get("response").and_then(extract_http_status_from_value)
        }
        _ => None,
    }
}

fn json_value_to_u16(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        serde_json::Value::String(s) => s.parse::<u16>().ok(),
        _ => None,
    }
}

fn find_header_case_insensitive(value: &serde_json::Value, header_name: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(headers) = map.get("headers") {
                if let Some(found) = find_header_case_insensitive(headers, header_name) {
                    return Some(found);
                }
            }

            if let Some(response) = map.get("response") {
                if let Some(found) = find_header_case_insensitive(response, header_name) {
                    return Some(found);
                }
            }

            map.iter().find_map(|(key, value)| {
                if key.eq_ignore_ascii_case(header_name) {
                    value.as_str().map(ToString::to_string)
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

fn auth_param_visible(header_value: &str, param_name: &str) -> bool {
    let lower = header_value.to_ascii_lowercase();
    let needle = format!("{param_name}=");

    lower
        .match_indices(&needle)
        .any(|(idx, _)| idx == 0 || matches!(lower.as_bytes()[idx - 1], b' ' | b',' | b'\t'))
}

fn normalize_jsonrpc_id(
    raw_id: Option<&serde_json::Value>,
    source_line: u64,
) -> Result<Option<String>> {
    match raw_id {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(id)) => Ok(Some(id.clone())),
        Some(serde_json::Value::Number(id)) => Ok(Some(id.to_string())),
        Some(serde_json::Value::Bool(_)) => {
            bail!(
                "JSON-RPC id on source line {} must not be a boolean",
                source_line
            )
        }
        Some(serde_json::Value::Array(_)) => {
            bail!(
                "JSON-RPC id on source line {} must not be an array",
                source_line
            )
        }
        Some(serde_json::Value::Object(_)) => {
            bail!(
                "JSON-RPC id on source line {} must not be an object",
                source_line
            )
        }
    }
}

fn validate_mcp_events(events: &[McpEvent]) -> Result<()> {
    let mut seen_tool_call_request_ids = HashSet::new();

    for event in events {
        if matches!(&event.payload, McpPayload::ToolCallRequest { .. }) {
            if let Some(id) = &event.jsonrpc_id {
                if !seen_tool_call_request_ids.insert(id.clone()) {
                    bail!(
                        "duplicate tools/call request id {:?} at source line {}",
                        id,
                        event.source_line
                    );
                }
            }
        }
    }

    Ok(())
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

#[cfg(test)]
mod era_wiring_tests;
