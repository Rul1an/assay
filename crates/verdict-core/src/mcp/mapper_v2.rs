use crate::mcp::types::*;
use crate::trace::schema::{EpisodeEnd, EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use serde_json::json;
use std::collections::HashMap;

/// Map normalized MCP events to Verdict V2 trace events (JSONL).
pub fn mcp_events_to_v2_trace(
    mut events: Vec<McpEvent>,
    episode_id: String,
    test_id: Option<String>,
    prompt_override: Option<String>,
) -> Vec<TraceEvent> {
    // P0.3: Deterministic Sort
    // 1. Timestamp (ms)
    // 2. Source Line (stable fallback)
    // 3. JSON-RPC ID (tie-breaker)
    events.sort_by(|a, b| {
        let ts_a = a.timestamp_ms.unwrap_or(0);
        let ts_b = b.timestamp_ms.unwrap_or(0);
        match ts_a.cmp(&ts_b) {
            std::cmp::Ordering::Equal => match a.source_line.cmp(&b.source_line) {
                std::cmp::Ordering::Equal => {
                    let id_a = a.jsonrpc_id.as_deref().unwrap_or("");
                    let id_b = b.jsonrpc_id.as_deref().unwrap_or("");
                    id_a.cmp(id_b)
                }
                other => other,
            },
            other => other,
        }
    });

    let start_ts = events
        .iter()
        .filter_map(|e| e.timestamp_ms)
        .min()
        .unwrap_or_else(now_ms);
    let mut out = Vec::new();

    // P0.1: Prompt Handling
    // If not provided, use sentinel to prevent CI failure (E_TRACE_MISS).
    let prompt_val = prompt_override.unwrap_or_else(|| "<mcp:session>".to_string());

    // EpisodeStart
    let mut meta = serde_json::Map::new();
    meta.insert("source".into(), json!("mcp_import"));
    meta.insert("episode_id".into(), json!(episode_id));

    if let Some(tid) = test_id {
        meta.insert("test_id".into(), json!(tid));
    }

    out.push(TraceEvent::EpisodeStart(EpisodeStart {
        episode_id: episode_id.clone(),
        timestamp: start_ts,
        input: json!({ "prompt": prompt_val }),
        meta: serde_json::Value::Object(meta),
    }));

    // P0.2: Correlation Buffer
    // Store pending tool calls: keys = jsonrpc_id
    // Values = (index in 'out', step_id, tool_name)
    // We emit the 'Step' immediately when Request comes, but we might update it later?
    // Actually, Verdict V2 separation of Step and ToolCall allows us to emit:
    // 1. Step (Request)
    // 2. ToolCall (Request + Response combined) -> Wait for Response.

    // BUT: To be strictly atomic and clean for the DB ingestion, it's better to emit both Step and ToolCall
    // when the *Response* arrives? No, Step usually marks the attempt start.

    // Better approach for "Atomic ToolCall" in V2:
    // V2 `ToolCallEntry` contains both `args` and `result`.
    // So we must wait for the Response to emit the `ToolCallEntry`.
    // We can emit the `StepEntry` (invocation) when the Request is seen, or wait.
    // Let's emit `StepEntry` on Request, and `ToolCallEntry` on Response.
    // The `ToolCallEntry` needs the `args` from the Request.

    // Map: id -> (StepId, ToolName, Args, Timestamp)
    let mut pending_calls: HashMap<String, (String, String, serde_json::Value, u64)> =
        HashMap::new();

    let mut idx: i64 = 0;
    let mut last_ts = start_ts;
    let mut final_output: Option<String> = None;

    for e in events {
        if let Some(ts) = e.timestamp_ms {
            last_ts = last_ts.max(ts);
        }

        match e.payload {
            McpPayload::ToolsListRequest { .. } => {
                out.push(TraceEvent::Step(StepEntry {
                    episode_id: episode_id.clone(),
                    step_id: format!("step_{:03}", idx),
                    idx: idx as u32,
                    timestamp: last_ts,
                    kind: "tool".to_string(),
                    name: Some("tools/list".to_string()),
                    content: Some("{}".to_string()),
                    content_sha256: None,
                    truncations: vec![],
                    meta: json!({}),
                }));
                idx += 1;
            }
            McpPayload::ToolsListResponse { tools, .. } => {
                out.push(TraceEvent::Step(StepEntry {
                    episode_id: episode_id.clone(),
                    step_id: format!("step_{:03}", idx),
                    idx: idx as u32,
                    timestamp: last_ts,
                    kind: "tool".to_string(),
                    name: Some("tools/list.result".to_string()),
                    content: Some(json!({ "tools": tools }).to_string()),
                    content_sha256: None,
                    truncations: vec![],
                    meta: json!({}),
                }));
                idx += 1;
            }
            McpPayload::ToolCallRequest {
                name, arguments, ..
            } => {
                let step_id = format!("step_{:03}", idx);

                // Emit the Step (invocation intent)
                out.push(TraceEvent::Step(StepEntry {
                    episode_id: episode_id.clone(),
                    step_id: step_id.clone(),
                    idx: idx as u32,
                    timestamp: last_ts,
                    kind: "tool".to_string(), // OTel mapping: system="tool"
                    name: Some(name.clone()),
                    content: None, // Content is in ToolCall args
                    content_sha256: None,
                    truncations: vec![],
                    meta: json!({ "jsonrpc_id": e.jsonrpc_id }),
                }));

                // Buffer for correlation if ID present
                if let Some(id) = e.jsonrpc_id {
                    pending_calls.insert(
                        id,
                        (step_id.clone(), name.clone(), arguments.clone(), last_ts),
                    );
                } else {
                    // Fire and forget / Notification?
                    // Emit incomplete ToolCall? Or just Step?
                    // Use Step ID as correlation fallback if needed.
                    out.push(TraceEvent::ToolCall(ToolCallEntry {
                        episode_id: episode_id.clone(),
                        step_id,
                        timestamp: last_ts,
                        tool_name: name,
                        call_index: Some(0),
                        args: arguments,
                        args_sha256: None,
                        result: None, // No response yet/ever
                        result_sha256: None,
                        error: None,
                        truncations: vec![],
                    }));
                }
                idx += 1;
            }
            McpPayload::ToolCallResponse {
                result, is_error, ..
            } => {
                // Try to find matching Request
                if let Some(id) = e.jsonrpc_id.clone() {
                    if let Some((step_id, name, args, _req_ts)) = pending_calls.remove(&id) {
                        // Found match! Emit complete ToolCall
                        out.push(TraceEvent::ToolCall(ToolCallEntry {
                            episode_id: episode_id.clone(),
                            step_id,
                            timestamp: last_ts,
                            tool_name: name,
                            call_index: Some(0),
                            args,
                            args_sha256: None,
                            result: Some(if is_error {
                                json!({"error": result.clone()})
                            } else {
                                result.clone()
                            }),
                            result_sha256: None,
                            error: if is_error {
                                Some("mcp_error".into())
                            } else {
                                None
                            },
                            truncations: vec![],
                        }));
                    } else {
                        // Orphan response: no matching pending tool call request
                        eprintln!(
                            "mcp_events_to_v2_trace: orphan ToolCallResponse with jsonrpc_id {:?} in episode {}",
                            id,
                            episode_id
                        );
                    }
                }
                // Heuristic: Last response is final output
                final_output = Some(result.to_string());
            }
            McpPayload::SessionEnd { .. } => {
                if final_output.is_none() {
                    final_output = Some("mcp_session_end".into());
                }
            }
            _ => {}
        }
    }

    // Flush pending calls (requests without responses)
    for (_id, (step_id, name, args, req_ts)) in pending_calls {
        out.push(TraceEvent::ToolCall(ToolCallEntry {
            episode_id: episode_id.clone(),
            step_id,
            timestamp: req_ts,
            tool_name: name,
            call_index: Some(0),
            args,
            args_sha256: None,
            result: None,
            result_sha256: None,
            error: Some("timeout/no_response".into()),
            truncations: vec![],
        }));
    }

    // Re-sort output by timestamp just in case buffering messed it up?
    // Steps are compliant, but late ToolCalls might appear "later" in stream.
    // But Trace V2 supports out-of-order ingestion usually.
    // However, `verdict-cli` replay usually expects roughly ordered stream.
    // Let's rely on the DB/Ingester to sort by timestamp if needed, but for local trace file, chronological is nice.
    // But `idx` is monotonic.

    out.push(TraceEvent::EpisodeEnd(EpisodeEnd {
        episode_id,
        timestamp: last_ts.max(start_ts),
        outcome: None,
        final_output,
    }));

    out
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
