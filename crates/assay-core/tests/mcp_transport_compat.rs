use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat, McpPayload};
use assay_core::trace::schema::TraceEvent;
use serde_json::{json, Value};

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

#[path = "mcp_transport_compat/semantic_normalization.rs"]
mod semantic_normalization;
#[path = "mcp_transport_compat/transport_contracts.rs"]
mod transport_contracts;
