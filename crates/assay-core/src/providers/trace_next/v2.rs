use crate::trace::schema::TraceEvent;
use std::collections::HashMap;

use super::parse::{EpisodeState, LineDisposition, ParsedTraceRecord};

/// The type of a native event line, or `None` for a row in an older form: no string `type`, or
/// `type: "assay.trace"`. Only `assay trace migrate` reads those (ADR-052). Replay and precompute
/// both decide with this function, so they refuse the same rows.
pub(crate) fn native_type(v: &serde_json::Value) -> Option<&str> {
    v.get("type")
        .and_then(|t| t.as_str())
        .filter(|t| *t != "assay.trace")
}

pub(crate) fn handle_typed_event(
    v: &serde_json::Value,
    active_episodes: &mut HashMap<String, EpisodeState>,
    parsed: &mut ParsedTraceRecord,
) -> LineDisposition {
    let Some(t) = native_type(v) else {
        return LineDisposition::Legacy;
    };

    match t {
        "episode_start" | "tool_call" | "episode_end" | "step" => {
            match serde_json::from_value::<TraceEvent>(v.clone()) {
                Ok(event) => apply_event(event, active_episodes, parsed),
                Err(_) => LineDisposition::Continue,
            }
        }
        _ => LineDisposition::Continue,
    }
}

fn apply_event(
    event: TraceEvent,
    active_episodes: &mut HashMap<String, EpisodeState>,
    parsed: &mut ParsedTraceRecord,
) -> LineDisposition {
    match event {
        TraceEvent::EpisodeStart(ev) => {
            let input_prompt = ev
                .input
                .get("prompt")
                .and_then(|s| s.as_str())
                .map(String::from);
            let has_input = input_prompt.is_some();
            let state = EpisodeState {
                input: input_prompt,
                output: None,
                model: None,
                meta: ev.meta,
                input_is_model: has_input,
                tool_calls: Vec::new(),
            };
            active_episodes.insert(ev.episode_id, state);

            LineDisposition::Continue
        }
        TraceEvent::ToolCall(ev) => {
            if let Some(state) = active_episodes.get_mut(&ev.episode_id) {
                state.tool_calls.push(crate::model::ToolCallRecord {
                    id: format!("{}-{}", ev.step_id, ev.call_index.unwrap_or(0)),
                    tool_name: ev.tool_name,
                    args: ev.args,
                    result: ev.result,
                    error: ev.error.map(serde_json::Value::String),
                    index: state.tool_calls.len(),
                    ts_ms: ev.timestamp,
                });
            }

            LineDisposition::Continue
        }
        TraceEvent::EpisodeEnd(ev) => {
            if let Some(mut state) = active_episodes.remove(&ev.episode_id) {
                if let Some(out) = ev.final_output {
                    state.output = Some(out);
                }
                if let Some(p) = state.input {
                    parsed.prompt = Some(p);
                    parsed.response = state.output;
                    merge_tool_calls_into_meta(&mut state.meta, &state.tool_calls);
                    parsed.meta = state.meta;
                    if let Some(model) = state.model {
                        parsed.model = model;
                    }
                }
            }

            LineDisposition::MaybeInsert
        }
        TraceEvent::Step(ev) => {
            if let Some(state) = active_episodes.get_mut(&ev.episode_id) {
                let is_model = ev.kind == "model";
                let can_extract_prompt = if is_model {
                    !state.input_is_model
                } else {
                    state.input.is_none()
                };

                if can_extract_prompt {
                    let mut found_prompt = None;
                    if let Some(c) = &ev.content {
                        if let Ok(c_json) = serde_json::from_str::<serde_json::Value>(c) {
                            if let Some(p) = c_json.get("prompt").and_then(|s| s.as_str()) {
                                found_prompt = Some(p.to_string());
                            }
                        }
                    }
                    if found_prompt.is_none() {
                        if let Some(p) = ev.meta.get("gen_ai.prompt").and_then(|s| s.as_str()) {
                            found_prompt = Some(p.to_string());
                        }
                    }
                    if let Some(p) = found_prompt {
                        state.input = Some(p);
                        if is_model {
                            state.input_is_model = true;
                        }
                    }
                }

                if let Some(c) = &ev.content {
                    let mut extracted = None;
                    if let Ok(c_json) = serde_json::from_str::<serde_json::Value>(c) {
                        if let Some((resp, model)) = crate::trace::schema::model_completion(&c_json)
                        {
                            extracted = Some(resp.to_string());
                            if let Some(m) = model {
                                state.model = Some(m.to_string());
                            }
                        }
                    }
                    if let Some(out) = extracted {
                        state.output = Some(out);
                    } else {
                        state.output = Some(c.clone());
                    }
                }

                if let Some(resp) = ev.meta.get("gen_ai.completion").and_then(|s| s.as_str()) {
                    state.output = Some(resp.to_string());
                }
                if let Some(m) = ev
                    .meta
                    .get("gen_ai.request.model")
                    .or(ev.meta.get("gen_ai.response.model"))
                    .and_then(|s| s.as_str())
                {
                    state.model = Some(m.to_string());
                }
            }

            LineDisposition::Continue
        }
    }
}

pub(crate) fn merge_tool_calls_into_meta(
    meta: &mut serde_json::Value,
    tool_calls: &[crate::model::ToolCallRecord],
) {
    if tool_calls.is_empty() {
        return;
    }

    let tool_calls_value = serde_json::to_value(tool_calls).unwrap_or_default();
    match meta {
        serde_json::Value::Object(map) => {
            map.insert("tool_calls".to_string(), tool_calls_value);
        }
        _ => {
            let mut map = serde_json::Map::new();
            map.insert("tool_calls".to_string(), tool_calls_value);
            *meta = serde_json::Value::Object(map);
        }
    }
}

// Match a completed replay response to the start row that carries its metadata.
// Precomputation requires a closed episode and an unambiguous source row.
pub(crate) fn precompute_episodes(
    source: &str,
) -> anyhow::Result<std::collections::BTreeMap<usize, (String, String)>> {
    use anyhow::{ensure, Context};
    use std::collections::{BTreeMap, HashSet};

    let mut active = HashMap::new();
    let mut starts = HashMap::new();
    let mut prompts = HashSet::new();
    let mut completed = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = super::parse::parse_trace_line_json(line, index + 1)?;
        if native_type(&value).is_none() {
            return Err(crate::providers::trace::TraceLoadError::LegacyReplay {
                line_no: index + 1,
            }
            .into());
        }
        let event: TraceEvent = serde_json::from_value(value)
            .with_context(|| format!("invalid episode event on line {}", index + 1))?;
        let start = match &event {
            TraceEvent::EpisodeStart(ev) => {
                ensure!(
                    starts.insert(ev.episode_id.clone(), index).is_none(),
                    "repeated open episode on line {}",
                    index + 1
                );
                None
            }
            TraceEvent::EpisodeEnd(ev) => {
                Some(starts.remove(&ev.episode_id).with_context(|| {
                    format!("episode end without a start on line {}", index + 1)
                })?)
            }
            TraceEvent::Step(ev) => {
                ensure!(
                    starts.contains_key(&ev.episode_id),
                    "step outside an episode on line {}",
                    index + 1
                );
                None
            }
            TraceEvent::ToolCall(ev) => {
                ensure!(
                    starts.contains_key(&ev.episode_id),
                    "tool call outside an episode on line {}",
                    index + 1
                );
                None
            }
        };
        let mut record = ParsedTraceRecord::new();
        apply_event(event, &mut active, &mut record);
        if let (Some(start), Some(prompt), Some(response)) = (start, record.prompt, record.response)
        {
            ensure!(
                prompts.insert(prompt.clone()),
                "duplicate replay prompt on line {}",
                index + 1
            );
            completed.insert(start, (prompt, response));
        }
    }
    ensure!(starts.is_empty(), "unfinished episode in precompute input");
    Ok(completed)
}
