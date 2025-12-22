use crate::trace::schema::{EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    #[serde(rename = "startTimeUnixNano")]
    pub start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    pub end_time_unix_nano: String,
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

/// Parse a string that might contain JSON. If parsing fails, return Value::String(original).
pub fn json_best_effort_str(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::String(String::new());
    }

    // Heuristic: only attempt JSON parse when it "looks like" JSON or primitives.
    let first = t.as_bytes()[0] as char;
    let looks_json = matches!(first, '{' | '[' | '"')
        || first.is_ascii_digit()
        || t == "true"
        || t == "false"
        || t == "null"
        || (first == '-' && t.len() > 1 && t.as_bytes()[1].is_ascii_digit());

    if looks_json {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return v;
        }
    }

    Value::String(s.to_string())
}

/// If an optional string is present, parse it best-effort into JSON Value.
pub fn json_best_effort_opt(s: Option<String>) -> Option<Value> {
    s.map(|x| json_best_effort_str(&x))
}

/// If you already have a Value but it might be a stringified JSON, normalize it.
pub fn json_best_effort_value(v: Value) -> Value {
    match v {
        Value::String(s) => json_best_effort_str(&s),
        other => other,
    }
}

fn normalize_attrs(mut meta: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = meta.as_object_mut() {
        for (_k, v) in obj.iter_mut() {
            // Turn `"{"a":1}"` into `{ "a": 1 }` when possible.
            *v = json_best_effort_value(v.take());
        }
    }
    meta
}

pub fn convert_spans_to_episodes(spans: Vec<OtelSpan>) -> Vec<TraceEvent> {
    // 1. Group by trace_id
    let mut by_trace: HashMap<String, Vec<OtelSpan>> = HashMap::new();
    for span in spans {
        by_trace
            .entry(span.trace_id.clone())
            .or_default()
            .push(span);
    }

    let mut out = Vec::new();

    for (trace_id, mut trace_spans) in by_trace {
        // 2. Sort: Start Time ASC, End Time DESC (widest first?), Span ID ASC
        trace_spans.sort_by(|a, b| {
            let start_a = a.start_time_unix_nano.parse::<u128>().unwrap_or(0);
            let start_b = b.start_time_unix_nano.parse::<u128>().unwrap_or(0);
            match start_a.cmp(&start_b) {
                std::cmp::Ordering::Equal => {
                    // Tie-break: End time DESC (parents usually encompass children)
                    let end_a = a.end_time_unix_nano.parse::<u128>().unwrap_or(0);
                    let end_b = b.end_time_unix_nano.parse::<u128>().unwrap_or(0);
                    match end_b.cmp(&end_a) {
                        // DESC
                        std::cmp::Ordering::Equal => a.span_id.cmp(&b.span_id),
                        ord => ord,
                    }
                }
                ord => ord,
            }
        });

        // Episode Start
        let start_ts = trace_spans
            .first()
            .map(|s| s.start_time_unix_nano.parse::<u64>().unwrap_or(0) / 1_000_000)
            .unwrap_or(0);

        // MVP: Assuming single root per trace_id is implicitly handled by grouping
        out.push(TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: trace_id.clone(),
            timestamp: start_ts,
            input: serde_json::Value::Null, // Optional input not readily available on root span yet
            meta: serde_json::json!({
                "source": "otel",
                "trace_id": trace_id
            }),
        }));

        let mut step_idx = 0;

        for span in trace_spans {
            let attrs = span.attributes.clone().unwrap_or_default();
            let attrs_value = serde_json::to_value(&attrs)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let meta = normalize_attrs(attrs_value);

            // Priority: gen_ai.operation.name
            let op_name = attrs
                .get("gen_ai.operation.name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let ts = span.start_time_unix_nano.parse::<u64>().unwrap_or(0) / 1_000_000;

            if op_name == "chat" || op_name == "text_completion" || op_name == "generate_content" {
                // Model Step
                step_idx += 1;
                let model = attrs
                    .get("gen_ai.request.model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                // Get raw values, let json_best_effort handle string vs json
                let prompt_raw = attrs.get("gen_ai.prompt").map(|v| v.to_string()); // to_string might escape quotes if it's already a string value?
                                                                                    // Wait, attrs.get returns &Value. If it IS a string Value::String("foo"), to_string gives "\"foo\"".
                                                                                    // We want the inner str if it is a string.
                let prompt_str = attrs
                    .get("gen_ai.prompt")
                    .and_then(|v| v.as_str())
                    .or_else(|| attrs.get("gen_ai.prompt").map(|_| "")); // simplified
                                                                         // Actually let's just use our helper on the Value directly if possible?
                                                                         // Helper takes &str.
                                                                         // Let's rely on our helper logic:
                let prompt_val = attrs.get("gen_ai.prompt").cloned().unwrap_or(Value::Null);
                let prompt_normalized = json_best_effort_value(prompt_val);

                let comp_val = attrs
                    .get("gen_ai.completion")
                    .cloned()
                    .unwrap_or(Value::Null);
                let comp_normalized = json_best_effort_value(comp_val);

                let content_json = serde_json::json!({
                    "model": model,
                    "prompt": prompt_normalized,
                    "completion": comp_normalized,
                });

                out.push(TraceEvent::Step(StepEntry {
                    episode_id: trace_id.clone(),
                    step_id: format!("{}-{}", trace_id, step_idx), // synthetic ID
                    idx: step_idx,
                    timestamp: ts,
                    kind: "model".to_string(),
                    name: Some(span.name.clone()),
                    content: Some(serde_json::to_string(&content_json).unwrap()),
                    meta: meta,
                    content_sha256: None,
                    truncations: vec![],
                }));
            } else if op_name == "execute_tool" {
                // Tool Create Step + ToolCall
                step_idx += 1;
                let step_id = format!("{}-{}", trace_id, step_idx);

                let tool_name = attrs
                    .get("gen_ai.tool.name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&span.name)
                    .to_string();

                // Step (stores meta/attrs)
                out.push(TraceEvent::Step(StepEntry {
                    episode_id: trace_id.clone(),
                    step_id: step_id.clone(),
                    idx: step_idx,
                    timestamp: ts,
                    kind: "tool".to_string(),
                    name: Some(span.name.clone()),
                    content: None,      // No text content for tool step usually
                    meta: meta.clone(), // Tool attrs live here
                    content_sha256: None,
                    truncations: vec![],
                }));

                // ToolCall (clean)
                let args_raw = attrs.get("gen_ai.tool.args");
                let args_val = match args_raw {
                    Some(v) => json_best_effort_value(v.clone()), // parses stringified json if needed
                    None => Value::Object(serde_json::Map::new()),
                };

                let result_raw = attrs.get("gen_ai.tool.result");
                let result_val = result_raw.map(|v| json_best_effort_value(v.clone()));

                out.push(TraceEvent::ToolCall(ToolCallEntry {
                    episode_id: trace_id.clone(),
                    step_id: step_id,
                    timestamp: ts,
                    tool_name: tool_name,
                    call_index: Some(0),
                    args: args_val,
                    result: result_val,
                    error: None,
                    args_sha256: None,
                    result_sha256: None,
                    truncations: vec![],
                }));
            } else if op_name == "invoke_agent" || op_name == "create_agent" {
                step_idx += 1;

                let content_json = serde_json::json!({
                    "operation": op_name,
                    "span_name": span.name
                });

                out.push(TraceEvent::Step(StepEntry {
                    episode_id: trace_id.clone(),
                    step_id: format!("{}-{}", trace_id, step_idx),
                    idx: step_idx,
                    timestamp: ts,
                    kind: "agent".to_string(),
                    name: Some(span.name.clone()),
                    content: Some(serde_json::to_string(&content_json).unwrap()),
                    meta: meta,
                    content_sha256: None,
                    truncations: vec![],
                }));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_json() {
        let v = json_best_effort_str(r#"{"a":1}"#);
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn parses_array_json() {
        let v = json_best_effort_str(r#"[1,2,3]"#);
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    #[test]
    fn keeps_plain_string() {
        let v = json_best_effort_str("hello");
        assert_eq!(v, serde_json::Value::String("hello".into()));
    }

    #[test]
    fn parses_boolean_null_number() {
        assert_eq!(json_best_effort_str("true"), serde_json::Value::Bool(true));
        assert_eq!(json_best_effort_str("null"), serde_json::Value::Null);
        assert_eq!(json_best_effort_str("12"), serde_json::json!(12));
        assert_eq!(json_best_effort_str("-7"), serde_json::json!(-7));
    }
}
