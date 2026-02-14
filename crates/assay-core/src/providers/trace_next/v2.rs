use std::collections::HashMap;

use super::parse::{EpisodeState, LineDisposition, ParsedTraceRecord};

pub(crate) fn handle_typed_event(
    v: &serde_json::Value,
    active_episodes: &mut HashMap<String, EpisodeState>,
    parsed: &mut ParsedTraceRecord,
) -> LineDisposition {
    let Some(t) = v.get("type").and_then(|t| t.as_str()) else {
        return LineDisposition::ParseLegacy;
    };

    match t {
        "assay.trace" => {
            parsed.prompt = v.get("prompt").and_then(|s| s.as_str()).map(String::from);
            parsed.response = v
                .get("response")
                .or(v.get("text"))
                .and_then(|s| s.as_str())
                .map(String::from);
            if let Some(m) = v.get("model").and_then(|s| s.as_str()) {
                parsed.model = m.to_string();
            }
            if let Some(m) = v.get("meta") {
                parsed.meta = m.clone();
            }
            if let Some(r) = v.get("request_id").and_then(|s| s.as_str()) {
                parsed.request_id = Some(r.to_string());
            }
            LineDisposition::MaybeInsert
        }
        "episode_start" => {
            if let Ok(ev) = serde_json::from_value::<crate::trace::schema::EpisodeStart>(v.clone())
            {
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
            }
            LineDisposition::Continue
        }
        "tool_call" => {
            if let Ok(ev) = serde_json::from_value::<crate::trace::schema::ToolCallEntry>(v.clone())
            {
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
            }
            LineDisposition::Continue
        }
        "episode_end" => {
            if let Ok(ev) = serde_json::from_value::<crate::trace::schema::EpisodeEnd>(v.clone()) {
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
            }
            LineDisposition::MaybeInsert
        }
        "step" => {
            if let Ok(ev) = serde_json::from_value::<crate::trace::schema::StepEntry>(v.clone()) {
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
                            if let Some(resp) = c_json.get("completion").and_then(|s| s.as_str()) {
                                extracted = Some(resp.to_string());
                                if let Some(m) = c_json.get("model").and_then(|s| s.as_str()) {
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
            }
            LineDisposition::Continue
        }
        _ => LineDisposition::Continue,
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
