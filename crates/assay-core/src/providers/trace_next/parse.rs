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
            meta: crate::trace::schema::default_replay_meta(),
            request_id: None,
        }
    }
}

pub(crate) enum LineDisposition {
    Continue,
    MaybeInsert,
    Legacy,
    /// A native event replay cannot read or place; the string is the reason.
    Refused(String),
}

pub(crate) fn parse_trace_line_json(
    line: &str,
    line_no: usize,
) -> anyhow::Result<serde_json::Value> {
    serde_json::from_str(line)
        .map_err(|e| anyhow::Error::new(errors::invalid_trace_format(line, line_no, &e)))
}

pub(crate) fn parse_legacy_record(v: &serde_json::Value) -> ParsedTraceRecord {
    let mut parsed = ParsedTraceRecord::new();
    parsed.prompt = v.get("prompt").and_then(|s| s.as_str()).map(String::from);
    parsed.response = v
        .get(crate::trace::schema::legacy_response_key(v))
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
    parsed
}

/// The unique-prompt rule: every episode that carries a prompt claims it, whether it ends with an
/// output, ends without one, or is still open at end of file. A second claim is a duplicate
/// prompt. An episode without an output adds no response, but skipping it must not free its
/// prompt for another episode, which would then replay in its place.
pub(crate) fn claim_prompt(claimed: &mut HashSet<String>, prompt: &str) -> anyhow::Result<()> {
    if !claimed.insert(prompt.to_string()) {
        return Err(anyhow::Error::new(errors::duplicate_prompt(prompt)));
    }
    Ok(())
}

pub(crate) fn insert_trace_record(
    traces: &mut HashMap<String, LlmResponse>,
    request_ids: &mut HashSet<String>,
    claimed_prompts: &mut HashSet<String>,
    parsed: ParsedTraceRecord,
    line_no: usize,
) -> anyhow::Result<()> {
    let Some(prompt) = parsed.prompt else {
        return Ok(());
    };
    let Some(response) = parsed.response else {
        // No response to replay, but the prompt is still this episode's.
        return claim_prompt(claimed_prompts, &prompt);
    };

    if let Some(rid) = &parsed.request_id {
        if request_ids.contains(rid) {
            return Err(anyhow::Error::new(errors::duplicate_request_id(
                line_no, rid,
            )));
        }
        request_ids.insert(rid.clone());
    }
    claim_prompt(claimed_prompts, &prompt)?;

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

/// Inserts the episodes still open at end of file. Every open prompt is claimed under the
/// unique-prompt rule before any episode is inserted, so the outcome does not depend on map order.
pub(crate) fn flush_active_episodes(
    traces: &mut HashMap<String, LlmResponse>,
    claimed_prompts: &mut HashSet<String>,
    active_episodes: HashMap<String, EpisodeState>,
) -> anyhow::Result<()> {
    for prompt in active_episodes.values().filter_map(|s| s.input.as_deref()) {
        claim_prompt(claimed_prompts, prompt)?;
    }
    for (_id, mut state) in active_episodes {
        if let (Some(p), Some(r)) = (state.input.clone(), state.output.clone()) {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::trace::TraceLoadError;

    fn record(request_id: &str, prompt: &str) -> ParsedTraceRecord {
        let mut parsed = ParsedTraceRecord::new();
        parsed.request_id = Some(request_id.to_string());
        parsed.prompt = Some(prompt.to_string());
        parsed.response = Some("hello".to_string());
        parsed
    }

    // A complete record that repeats both a request_id and a prompt reports the request_id, as
    // it did before the unique-prompt rule moved into claim_prompt.
    #[test]
    fn a_repeated_request_id_is_reported_before_a_repeated_prompt() {
        let mut traces = HashMap::new();
        let mut request_ids = HashSet::new();
        let mut claimed = HashSet::new();
        insert_trace_record(
            &mut traces,
            &mut request_ids,
            &mut claimed,
            record("greeting-1", "greeting"),
            1,
        )
        .expect("first record");
        let err = insert_trace_record(
            &mut traces,
            &mut request_ids,
            &mut claimed,
            record("greeting-1", "greeting"),
            2,
        )
        .expect_err("second record refused");
        assert!(
            matches!(
                err.downcast_ref::<TraceLoadError>(),
                Some(TraceLoadError::DuplicateRequestId { line_no: 2, request_id }) if request_id == "greeting-1"
            ),
            "{err}"
        );
    }
}
