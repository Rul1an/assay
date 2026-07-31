use crate::mcp::era::{
    classify_message, correlation_id, fold_envelope, id_is_acceptable, observe_client_capabilities,
    observe_header, observe_request_metadata, observe_result, resolve_era, CapabilityObservation,
    EnvelopeObservation, McpEraContext, MessageKind, ParsedMcpEvent, RequestMetadata,
};
use crate::mcp::era::{CorrelationId, DuplicateAwareSink, EraResolution, SeenMembers, UniqueValue};
use crate::mcp::types::*;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

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
    let (events, envelopes) = parse_events_with_envelopes(text, format)?;
    let framed = is_framed(format);
    let parsed: Vec<ParsedMcpEvent> = events
        .into_iter()
        .map(|event| {
            // Observations are taken by reference before the event moves, so no payload is cloned.
            //
            // One classification decides both axes, so neither can be reached from the other's
            // arm. Reading `result` unconditionally gave a hybrid message, a valid string `method`
            // alongside a `result`, both request metadata and a result observation: a result
            // conclusion about an event the parser had already called a request. And a notification
            // carries `method` like a request while its `_meta` is optional and a different type,
            // so holding it to the request requirement invents a fault in the other direction.
            //
            // The capability set travels on the same arm as the request metadata and for the same
            // reason: it is stated by a request and nowhere else. A response gets it by correlation
            // below, never by reading its own bytes.
            let (request_metadata, result_observation, capability_observation, is_error_response) =
                match payload_raw(&event.payload) {
                    // The same classifier the parser used. A shape it rejects never reaches here,
                    // because `parse_events_with_envelopes` runs first and refuses it, so the error
                    // arm is a state this pass cannot observe rather than one it tolerates.
                    Some(raw) => match classify_message(raw) {
                        Ok(MessageKind::Request { .. }) => (
                            Some(observe_request_metadata(raw)),
                            None,
                            observe_client_capabilities(raw),
                            false,
                        ),
                        Ok(MessageKind::Notification { .. }) => (None, None, None, false),
                        Ok(MessageKind::Response) => {
                            (None, observe_result(raw), None, raw.get("error").is_some())
                        }
                        Err(_) => (None, None, None, false),
                    },
                    None => (None, None, None, false),
                };
            // Unframed formats have one whole-input observation and no entries to index. A framed
            // input that does not resolve to an entry is a mapping the parser cannot vouch for,
            // and a plausible-but-wrong attribution is worse than an unusable one.
            let envelope = if framed {
                envelopes
                    .get(event.source_line.saturating_sub(1) as usize)
                    .cloned()
                    .unwrap_or(EnvelopeObservation::Malformed)
            } else {
                EnvelopeObservation::NotApplicable
            };
            let era = resolve_era(
                &envelope,
                request_metadata
                    .as_ref()
                    .unwrap_or(&RequestMetadata::Absent),
            );
            // The typed key, taken from the same reader `correlate_calls` keys on, so the sidecar
            // and the correlation cannot disagree about which call this is.
            let correlation = payload_raw(&event.payload).and_then(correlation_id);
            ParsedMcpEvent {
                event,
                context: McpEraContext {
                    envelope,
                    era,
                    correlation,
                    request_metadata,
                    result_observation,
                    capability_observation,
                },
                is_error_response,
            }
        })
        .collect();
    correlate_calls(parsed)
}

/// What one outstanding call carries forward to its own response.
struct CallSignals {
    era: EraResolution,
    capability: Option<CapabilityObservation>,
}

/// Give a response the era and capability set its own call resolved to.
///
/// The era resolves from two signals that both live on a request: the transport header and
/// `params._meta`. A response carries neither, so it fell back to the header alone. A request whose
/// header and body disagree is `Conflicting`, while its response resolved to `Known` from the header
/// and a missing `resultType` under a legacy era is `Terminal` — so a contradicted call could still
/// conclude that the action completed. The contradiction has to travel to the result.
///
/// The capability set travels the same way and for a sharper reason: it is stated per request and
/// MUST NOT be inferred from a prior one, so reading it off anything but this call's own request
/// would be the inference the revision forbids.
///
/// Correlation is by JSON-RPC id within one transcript, which the parser already establishes and
/// validates for duplicates. A response with no matching request keeps the era it resolved on its
/// own, so this adds authority rather than removing it. Multi-hop calls spread across separate
/// records stay out of scope: that needs a call-scoped identity this slice does not define.
fn correlate_calls(mut parsed: Vec<ParsedMcpEvent>) -> Result<Vec<ParsedMcpEvent>> {
    // Source order, not a map built up front. A global map is last-wins, so a response could take
    // the era of a request that had not happened yet: with an id reused after the response, the
    // contradiction on the call being answered was replaced by the clean era of the next call.
    // Requests are outstanding until a response consumes them.
    //
    // Era and capability travel together on one entry rather than in two maps. They are two facts
    // about the same call, and two maps could be removed at different moments and give a response
    // one call's era with another call's capabilities.
    let mut outstanding: std::collections::HashMap<CorrelationId, CallSignals> =
        std::collections::HashMap::new();
    for p in &mut parsed {
        // The typed key, not the public `String` rendering: that renders JSON `1` and `"1"`
        // identically, and they are different ids.
        let Some(raw) = payload_raw(&p.event.payload) else {
            continue;
        };
        let Some(id) = correlation_id(raw) else {
            continue;
        };
        // The shared classifier is the authority, not the presence of an observation. An error
        // response deliberately has no result observation, since the `resultType` requirement is
        // about `result`, so keying off that observation made an error response invisible here: it
        // inherited nothing and consumed nothing, and a legal sequential reuse of its id then
        // tripped the two-outstanding refusal.
        let kind = classify_message(raw).ok();
        match kind {
            Some(MessageKind::Request { .. }) => {
                // Two calls outstanding on one id makes the correlation ambiguous, and choosing
                // either is a silent choice between two calls. Reuse after a response is legal and
                // is what the removal below permits.
                let signals = CallSignals {
                    era: p.context.era.clone(),
                    capability: p.context.capability_observation.clone(),
                };
                if outstanding.insert(id.clone(), signals).is_some() {
                    bail!(
                        "two outstanding JSON-RPC requests share an id at source line {}",
                        p.event.source_line
                    );
                }
            }
            Some(MessageKind::Response) => {
                // An orphan response keeps the era it resolved on its own, so correlation adds
                // authority rather than removing it. Its capability observation stays `None`: no
                // request was seen, and borrowing a neighbouring call's set is exactly the
                // inference the revision forbids.
                if let Some(signals) = outstanding.remove(&id) {
                    p.context.era = signals.era;
                    p.context.capability_observation = signals.capability;
                }
            }
            Some(MessageKind::Notification { .. }) | None => {}
        }
    }
    Ok(parsed)
}

/// Read the header slot inside a `transport_context` as an observation.
///
/// A container that is present and not an object is a signal that arrived and failed, not silence.
/// `Value::get` answers `None` for a scalar, an array or a null, which made a deviant container
/// indistinguishable from no container at all: at transcript level the whole transcript read as
/// `Absent`, and at entry level the entry silently inherited whatever valid default the transcript
/// had set. Both are a fold toward "nothing was wrong" on the evidence that something was.
fn observe_transport_context(ctx: Option<&serde_json::Value>) -> Option<EnvelopeObservation> {
    let ctx = ctx?;
    let Some(map) = ctx.as_object() else {
        return Some(EnvelopeObservation::Malformed);
    };
    // A readable container with no `headers` key is silence rather than a defect: it arrived, it
    // was legible, and it carried no header slot. Only an unreadable one is a finding.
    observe_header(map.get("headers"))
}

/// Which formats carry transport framing at all. `Inspector` reads an events array straight into
/// `parse_jsonrpc_message` and never reaches `parse_transport_transcript`, so it has no envelope.
fn is_framed(format: McpInputFormat) -> bool {
    matches!(
        format,
        McpInputFormat::StreamableHttp | McpInputFormat::HttpSse
    )
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

fn parse_events_with_envelopes(
    text: &str,
    format: McpInputFormat,
) -> Result<(Vec<McpEvent>, Vec<EnvelopeObservation>)> {
    let (events, envelopes) = match format {
        McpInputFormat::JsonRpc => (parse_jsonrpc_jsonl(text)?, Vec::new()),
        McpInputFormat::Inspector => (parse_inspector_best_effort(text)?, Vec::new()),
        McpInputFormat::StreamableHttp => parse_transport_transcript_detailed(
            text,
            "streamable-http",
            "streamable-http transcript",
            false,
        )?,
        McpInputFormat::HttpSse => {
            parse_transport_transcript_detailed(text, "http-sse", "http-sse transcript", true)?
        }
    };
    // No global id gate here. The old one keyed on the public `String` rendering, so a number `1`
    // and a string `"1"` collided; it refused any reuse anywhere in the transcript, so a legal reuse
    // after a response was rejected; and it only saw `ToolCallRequest`. It also ran before
    // correlation, which made it the de facto lifetime authority without owning the typed key.
    // `correlate_calls` is that authority now, on the typed outstanding map.
    Ok((events, envelopes))
}

fn parse_jsonrpc_jsonl(text: &str) -> Result<Vec<McpEvent>> {
    let mut out = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let UniqueValue(v) = serde_json::from_str::<UniqueValue>(line)
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
    let UniqueValue(v) =
        serde_json::from_str::<UniqueValue>(text).context("invalid inspector JSON")?;

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

/// The transport parse, returning the per-entry envelope observations it already had in hand.
///
/// Deserializing the text a second time to read the headers would double the peak memory a hostile
/// transcript can cost, on the one path that exists to read untrusted input, so the observations
/// are taken from the same `TransportTranscript` the events come from.
fn parse_transport_transcript_detailed(
    text: &str,
    expected_transport: &str,
    source_label: &str,
    allow_endpoint_event: bool,
) -> Result<(Vec<McpEvent>, Vec<EnvelopeObservation>)> {
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

    let mut transcript_slots = Vec::new();
    if let Some(o) = observe_transport_context(transcript.transport_context.as_ref().map(|u| &u.0))
    {
        transcript_slots.push(o);
    }
    if let Some(o) = observe_header(transcript.headers.as_ref().map(|u| &u.0)) {
        transcript_slots.push(o);
    }

    let mut envelopes = Vec::new();
    let mut out = Vec::new();
    for (idx, entry) in transcript.entries.into_iter().enumerate() {
        let mut slots = transcript_slots.clone();
        if let Some(o) = observe_transport_context(entry.transport_context.as_ref().map(|u| &u.0)) {
            slots.push(o);
        }
        if let Some(o) = observe_header(entry.headers.as_ref().map(|u| &u.0)) {
            slots.push(o);
        }
        envelopes.push(fold_envelope(slots, true));
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

        if let Some(UniqueValue(request)) = entry.request {
            out.push(parse_jsonrpc_message(
                request,
                source_line,
                entry.timestamp_ms,
                McpAuthorizationDiscovery::default(),
            )?);
            continue;
        }

        let auth_discovery = parse_transport_auth_discovery(&entry);

        if let Some(UniqueValue(response)) = entry.response {
            out.push(parse_jsonrpc_message(
                response,
                source_line,
                entry.timestamp_ms,
                auth_discovery,
            )?);
            continue;
        }

        if let Some(sse) = entry.sse {
            if let Some(jsonrpc) = extract_jsonrpc_from_sse(&sse, allow_endpoint_event)? {
                out.push(parse_jsonrpc_message(
                    jsonrpc,
                    source_line,
                    entry.timestamp_ms,
                    McpAuthorizationDiscovery::default(),
                )?);
            }
        }
    }

    Ok((out, envelopes))
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

    // One classification for the whole crate. A present non-string `method` is a malformed message
    // shape, not a message of another kind, and it is refused here: that is a JSON-RPC shape
    // refusal rather than an era-state refusal, so it does not weaken the rule that every era
    // observation parses. The message is value-free.
    let kind = classify_message(&v)
        .map_err(|e| anyhow::anyhow!("MCP event at source line {}: {}", source_line, e))?;

    // Classification first, then the id its kind requires. The shapes differ: a notification has
    // no id by definition, a request and a success response must name the call they are part of,
    // and an error response is how a peer reports a request it could not parse, so it may have no
    // usable id at all and simply correlates with nothing. Normalizing before classifying cannot
    // apply any of that, because it does not yet know what it is looking at.
    let raw_id = v.get("id");
    let id_str = match kind {
        MessageKind::Notification { .. } => None,
        MessageKind::Request { .. } => Some(require_acceptable_id(raw_id, source_line)?),
        MessageKind::Response => {
            let is_error = v.get("error").is_some();
            match raw_id {
                // An invalid-request error response may carry no usable id.
                None | Some(serde_json::Value::Null) if is_error => None,
                _ => Some(require_acceptable_id(raw_id, source_line)?),
            }
        }
    };
    let payload =
        if let MessageKind::Request { method } | MessageKind::Notification { method } = kind {
            match method {
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
        .and_then(|value| find_header_case_insensitive(&value.0, "www-authenticate"))
        .or_else(|| {
            entry
                .headers
                .as_ref()
                .and_then(|value| find_header_case_insensitive(&value.0, "www-authenticate"))
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
        .and_then(|v| extract_http_status_from_value(&v.0))
        .or_else(|| {
            entry
                .headers
                .as_ref()
                .and_then(|v| extract_http_status_from_value(&v.0))
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

/// Accept the id a request or a success response must carry, or refuse value-free.
///
/// The existing type diagnostics for booleans, arrays and objects are kept: they are more specific
/// than "not a string or a number" and downstream tests pin them.
fn require_acceptable_id(raw_id: Option<&serde_json::Value>, source_line: u64) -> Result<String> {
    // Acceptance and correlation are different questions, so this asks only the first. One clone,
    // on the accepting path; the refusing path copies nothing.
    reject_unusable_id_shape(raw_id, source_line)?;
    // Acceptance is broader than correlation and is decided here: the schema allows a string or any
    // number, so any of those is an acceptable id even when `correlation_id` declines to key it.
    // Re-deciding the *correlation* rule here is what once made its mutation stop biting, so this
    // asks only about acceptance and leaves keying to the one function that owns it.
    match raw_id.filter(|v| id_is_acceptable(v)) {
        Some(serde_json::Value::String(id)) => Ok(id.clone()),
        Some(serde_json::Value::Number(id)) => Ok(id.to_string()),
        _ => bail!(
            "JSON-RPC id on source line {} must be a string or a number",
            source_line
        ),
    }
}

/// The more specific type diagnostics, kept because they name the fault better than "not a string or
/// an integer" and downstream tests pin them. Returns nothing: the accepted value is produced by the
/// single match in [`require_acceptable_id`], so this no longer clones one to throw it away.
fn reject_unusable_id_shape(raw_id: Option<&serde_json::Value>, source_line: u64) -> Result<()> {
    match raw_id {
        None
        | Some(serde_json::Value::Null)
        | Some(serde_json::Value::String(_))
        | Some(serde_json::Value::Number(_)) => Ok(()),
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

fn extract_jsonrpc_from_sse(
    sse: &TransportSseEnvelope,
    allow_endpoint_event: bool,
) -> Result<Option<serde_json::Value>> {
    let event_name = sse.event.as_deref().unwrap_or("message");
    if event_name == "endpoint" && allow_endpoint_event {
        return Ok(None);
    }

    if event_name != "message" {
        return Ok(None);
    }

    extract_jsonrpc_like_value(&sse.data.0)
}

/// Pull a JSON-RPC-looking value out of an SSE `data` payload.
///
/// `Ok(None)` means the payload is not JSON-RPC-shaped, which is tolerated: an SSE stream carries
/// keepalives and endpoint frames alongside messages. An `Err` means the payload *is* JSON and
/// carries a duplicate member, which is refused. The two are told apart by
/// `serde_json::error::Category`, not by reading the message: a visitor's `Error::custom` classifies
/// as `Data` while malformed bytes classify as `Syntax`, so the distinction is typed.
fn extract_jsonrpc_like_value(value: &serde_json::Value) -> Result<Option<serde_json::Value>> {
    match value {
        serde_json::Value::Object(map)
            if map.contains_key("method")
                || map.contains_key("result")
                || map.contains_key("error")
                || map.contains_key("jsonrpc") =>
        {
            Ok(Some(value.clone()))
        }
        // The embedded string is a *different* input from the transcript, not a second pass over the
        // same one, so it goes through the same duplicate-aware boundary rather than a plain
        // `Value`. Without this an SSE frame carrying its payload as a string was the one path that
        // kept a duplicate member.
        serde_json::Value::String(text) => match serde_json::from_str::<UniqueValue>(text) {
            Ok(UniqueValue(parsed)) => extract_jsonrpc_like_value(&parsed),
            Err(e) if e.classify() == serde_json::error::Category::Data => {
                Err(anyhow::Error::new(e).context("invalid SSE data payload"))
            }
            Err(_) => Ok(None),
        },
        _ => Ok(None),
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

#[derive(Debug, Default)]
struct TransportTranscript {
    transport: Option<String>,
    #[allow(dead_code)]
    transport_context: Option<UniqueValue>,
    #[allow(dead_code)]
    headers: Option<UniqueValue>,
    entries: Vec<TransportTranscriptEntry>,
}

/// Hand-written so every member name passes through [`SeenMembers`], including the unknown ones the
/// derive discards without looking at. Assigning `Some(..)` on each arm keeps the property the
/// deleted `present_slot` helper carried: an explicitly written `null` stays present rather than
/// folding to absent, which is what let a broken entry inherit a valid transcript default.
impl<'de> Deserialize<'de> for TransportTranscript {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = TransportTranscript;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a transport transcript with unique members")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<TransportTranscript, A::Error> {
                let mut out = TransportTranscript::default();
                let mut seen = SeenMembers::default();
                while let Some(key) = map.next_key::<String>()? {
                    seen.insert::<A::Error>(&key)?;
                    match key.as_str() {
                        "transport" => out.transport = map.next_value()?,
                        "transport_context" => out.transport_context = Some(map.next_value()?),
                        "headers" => out.headers = Some(map.next_value()?),
                        "entries" => out.entries = map.next_value()?,
                        _ => {
                            map.next_value::<DuplicateAwareSink>()?;
                        }
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Debug, Default)]
struct TransportTranscriptEntry {
    timestamp_ms: Option<u64>,
    #[allow(dead_code)]
    transport_context: Option<UniqueValue>,
    #[allow(dead_code)]
    headers: Option<UniqueValue>,
    request: Option<UniqueValue>,
    response: Option<UniqueValue>,
    sse: Option<TransportSseEnvelope>,
}

/// Same reason as the transcript above.
impl<'de> Deserialize<'de> for TransportTranscriptEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = TransportTranscriptEntry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a transport transcript entry with unique members")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<TransportTranscriptEntry, A::Error> {
                let mut out = TransportTranscriptEntry::default();
                let mut seen = SeenMembers::default();
                while let Some(key) = map.next_key::<String>()? {
                    seen.insert::<A::Error>(&key)?;
                    match key.as_str() {
                        "timestamp_ms" => out.timestamp_ms = map.next_value()?,
                        "transport_context" => out.transport_context = Some(map.next_value()?),
                        "headers" => out.headers = Some(map.next_value()?),
                        "request" => out.request = Some(map.next_value()?),
                        "response" => out.response = Some(map.next_value()?),
                        // Symmetric with the other slots: an explicitly written null is a slot
                        // that arrived and failed, not an absent one. Folding it to absent let it
                        // vanish silently beside a valid request.
                        "sse" => match map.next_value::<Option<TransportSseEnvelope>>()? {
                            Some(envelope) => out.sse = Some(envelope),
                            None => {
                                return Err(serde::de::Error::custom(
                                    "sse slot is present and null",
                                ))
                            }
                        },
                        _ => {
                            map.next_value::<DuplicateAwareSink>()?;
                        }
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Debug, Default)]
struct TransportSseEnvelope {
    event: Option<String>,
    #[allow(dead_code)]
    id: Option<String>,
    data: UniqueValue,
}

/// Duplicate-aware like the transcript and the entry. The derive would have let a repeated unknown
/// member through, and `data` is where an SSE frame carries its whole JSON-RPC message.
impl<'de> Deserialize<'de> for TransportSseEnvelope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = TransportSseEnvelope;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an SSE envelope with unique members")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<TransportSseEnvelope, A::Error> {
                let mut out = TransportSseEnvelope::default();
                let mut seen = SeenMembers::default();
                // A hand-written `Deserialize` inherits none of the derive's field obligations. The
                // derive made `data` required; initializing from `Default` and never checking let a
                // frame with no data produce zero events instead of a refusal, which is a malformed
                // frame disappearing silently.
                let mut saw_data = false;
                while let Some(key) = map.next_key::<String>()? {
                    seen.insert::<A::Error>(&key)?;
                    match key.as_str() {
                        "event" => out.event = map.next_value()?,
                        "id" => out.id = map.next_value()?,
                        "data" => {
                            out.data = map.next_value()?;
                            saw_data = true;
                        }
                        _ => {
                            map.next_value::<DuplicateAwareSink>()?;
                        }
                    }
                }
                if !saw_data {
                    return Err(serde::de::Error::missing_field("data"));
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}

#[cfg(test)]
mod era_wiring_tests;
