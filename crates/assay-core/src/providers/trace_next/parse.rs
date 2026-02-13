use std::collections::{HashMap, HashSet};

use super::{errors, v2};
use crate::model::LlmResponse;

pub(crate) struct EpisodeState {
    pub(crate) input: Option<String>,
    pub(crate) output: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) meta: serde_json::Value,
    pub(crate) input_is_model: bool,
    pub(crate) tool_calls: Vec<crate::model::ToolCallRecord>,
}

pub(crate) struct ParsedTraceRecord {
    pub(crate) prompt: Option<String>,
    pub(crate) response: Option<String>,
    pub(crate) model: String,
    pub(crate) meta: serde_json::Value,
    pub(crate) request_id: Option<String>,
}

impl ParsedTraceRecord {
    pub(crate) fn new() -> Self {
        Self {
            prompt: None,
            response: None,
            model: "trace".to_string(),
            meta: serde_json::json!({}),
            request_id: None,
        }
    }
}

pub(crate) enum LineDisposition {
    Continue,
    MaybeInsert,
    ParseLegacy,
}

pub(crate) fn parse_trace_line_json(
    line: &str,
    line_no: usize,
) -> anyhow::Result<serde_json::Value> {
    serde_json::from_str(line).map_err(|e| errors::invalid_trace_format(line, line_no, &e))
}

pub(crate) fn parse_legacy_record(v: &serde_json::Value, parsed: &mut ParsedTraceRecord) {
    parsed.prompt = v.get("prompt").and_then(|s| s.as_str()).map(String::from);
    parsed.response = v
        .get("response")
        .or(v.get("text"))
        .and_then(|s| s.as_str())
        .map(String::from);
    if let Some(m) = v.get("model").and_then(|s| s.as_str()) {
        parsed.model = m.to_string();
    }
    if let Some(r) = v.get("request_id").and_then(|s| s.as_str()) {
        parsed.request_id = Some(r.to_string());
    }

    let tool_name = v.get("tool").and_then(|s| s.as_str()).map(String::from);
    let tool_args = v.get("args").cloned();
    let has_tool_calls = v.get("tool_calls").and_then(|v| v.as_array()).is_some();
    let has_tool_signal = tool_name.is_some() || has_tool_calls;
    if let Some(tool) = tool_name {
        let record = crate::model::ToolCallRecord {
            id: "legacy-v1".to_string(),
            tool_name: tool,
            args: tool_args.unwrap_or(serde_json::json!({})),
            result: None,
            error: None,
            index: 0,
            ts_ms: 0,
        };
        parsed.meta["tool_calls"] = serde_json::json!([record]);
    } else if let Some(calls) = v.get("tool_calls").and_then(|v| v.as_array()) {
        parsed.meta["tool_calls"] = serde_json::Value::Array(calls.clone());
    }

    if has_tool_signal && parsed.prompt.is_none() {
        parsed.prompt = Some("ignore".to_string());
    }

    if has_tool_signal && parsed.response.is_none() {
        let response = match v.get("result") {
            Some(result) => result
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| result.to_string()),
            None => String::new(),
        };
        parsed.response = Some(response);
    }
}

pub(crate) fn insert_trace_record(
    traces: &mut HashMap<String, LlmResponse>,
    request_ids: &mut HashSet<String>,
    parsed: ParsedTraceRecord,
    line_no: usize,
) -> anyhow::Result<()> {
    let (Some(prompt), Some(response)) = (parsed.prompt, parsed.response) else {
        return Ok(());
    };

    if let Some(rid) = &parsed.request_id {
        if request_ids.contains(rid) {
            return Err(errors::duplicate_request_id(line_no, rid));
        }
        request_ids.insert(rid.clone());
    }

    if traces.contains_key(&prompt) {
        return Err(errors::duplicate_prompt(&prompt));
    }

    traces.insert(
        prompt,
        LlmResponse {
            text: response,
            meta: parsed.meta,
            model: parsed.model,
            provider: "trace".to_string(),
            ..Default::default()
        },
    );

    Ok(())
}

pub(crate) fn flush_active_episodes(
    traces: &mut HashMap<String, LlmResponse>,
    active_episodes: HashMap<String, EpisodeState>,
) {
    for (id, mut state) in active_episodes {
        if let (Some(p), Some(r)) = (state.input.clone(), state.output.clone()) {
            if traces.contains_key(&p) {
                eprintln!("Warning: Duplicate prompt skipped at EOF for id {}", id);
                continue;
            }
            v2::merge_tool_calls_into_meta(&mut state.meta, &state.tool_calls);
            traces.insert(
                p,
                LlmResponse {
                    text: r,
                    meta: state.meta,
                    model: state.model.unwrap_or_else(|| "trace".to_string()),
                    provider: "trace".to_string(),
                    ..Default::default()
                },
            );
        }
    }
}
