use crate::errors::{diagnostic::codes, similarity::closest_prompt, Diagnostic};
use crate::model::LlmResponse;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct TraceClient {
    // prompts -> response
    traces: Arc<HashMap<String, LlmResponse>>,
    fingerprint: String,
}
struct EpisodeState {
    input: Option<String>,
    output: Option<String>,
    model: Option<String>,
    meta: serde_json::Value,
    input_is_model: bool,
    tool_calls: Vec<crate::model::ToolCallRecord>,
}

struct ParsedTraceRecord {
    prompt: Option<String>,
    response: Option<String>,
    model: String,
    meta: serde_json::Value,
    request_id: Option<String>,
}

impl ParsedTraceRecord {
    fn new() -> Self {
        Self {
            prompt: None,
            response: None,
            model: "trace".to_string(),
            meta: serde_json::json!({}),
            request_id: None,
        }
    }
}

enum LineDisposition {
    Continue,
    MaybeInsert,
    ParseLegacy,
}

fn parse_trace_line_json(line: &str, line_no: usize) -> anyhow::Result<serde_json::Value> {
    serde_json::from_str(line).map_err(|e| {
        anyhow::anyhow!(
            "line {}: Invalid trace format. Expected JSONL object.\n  Error: {}\n  Content: {}",
            line_no,
            e,
            line.chars().take(50).collect::<String>()
        )
    })
}

fn handle_typed_event(
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

fn merge_tool_calls_into_meta(
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

fn parse_legacy_record(v: &serde_json::Value, parsed: &mut ParsedTraceRecord) {
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

    // Legacy tool-only fixtures (tool/args/result without prompt/response) are common
    // in policy demos. Canonicalize to a deterministic fallback lookup key so
    // `input: "ignore"` works out of the box.
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

fn insert_trace_record(
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
            return Err(anyhow::anyhow!(
                "line {}: Duplicate request_id {}",
                line_no,
                rid
            ));
        }
        request_ids.insert(rid.clone());
    }

    if traces.contains_key(&prompt) {
        return Err(anyhow::anyhow!(
            "Duplicate prompt found in trace file: {}",
            prompt
        ));
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

fn flush_active_episodes(
    traces: &mut HashMap<String, LlmResponse>,
    active_episodes: HashMap<String, EpisodeState>,
) {
    for (id, mut state) in active_episodes {
        if let (Some(p), Some(r)) = (state.input.clone(), state.output.clone()) {
            if traces.contains_key(&p) {
                eprintln!("Warning: Duplicate prompt skipped at EOF for id {}", id);
                continue;
            }
            merge_tool_calls_into_meta(&mut state.meta, &state.tool_calls);
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

fn compute_trace_fingerprint(traces: &HashMap<String, LlmResponse>) -> String {
    let mut keys: Vec<&String> = traces.keys().collect();
    keys.sort();
    let mut hasher = sha2::Sha256::new();
    for k in keys {
        hasher.update(k.as_bytes());
        if let Some(v) = traces.get(k) {
            hasher.update(v.text.as_bytes());
            hasher.update(v.model.as_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

impl TraceClient {
    pub fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let file = File::open(path.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "failed to open trace file '{}': {}",
                path.as_ref().display(),
                e
            )
        })?;
        let reader = std::io::BufReader::new(file);

        let mut traces = HashMap::new();
        let mut request_ids = HashSet::new();
        let mut active_episodes: HashMap<String, EpisodeState> = HashMap::new();

        for (i, line_res) in reader.lines().enumerate() {
            let line_no = i + 1;
            let line = line_res?;
            if line.trim().is_empty() {
                continue;
            }

            let v = parse_trace_line_json(&line, line_no)?;
            let mut parsed = ParsedTraceRecord::new();

            match handle_typed_event(&v, &mut active_episodes, &mut parsed) {
                LineDisposition::Continue => continue,
                LineDisposition::MaybeInsert => {}
                LineDisposition::ParseLegacy => parse_legacy_record(&v, &mut parsed),
            }

            insert_trace_record(&mut traces, &mut request_ids, parsed, line_no)?;
        }

        flush_active_episodes(&mut traces, active_episodes);
        let fingerprint = compute_trace_fingerprint(&traces);

        Ok(Self {
            traces: Arc::new(traces),
            fingerprint,
        })
    }
}

#[async_trait]
impl LlmClient for TraceClient {
    async fn complete(
        &self,
        prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        if let Some(resp) = self.traces.get(prompt) {
            Ok(resp.clone())
        } else {
            // Find closest match for hint
            let closest = closest_prompt(prompt, self.traces.keys());

            let mut diag = Diagnostic::new(
                codes::E_TRACE_MISS,
                "Trace miss: prompt not found in loaded traces".to_string(),
            )
            .with_source("trace")
            .with_context(serde_json::json!({
                "prompt": prompt,
                "closest_match": closest
            }));

            if let Some(match_) = closest {
                diag = diag.with_fix_step(format!(
                    "Did you mean '{}'? (similarity: {:.2})",
                    match_.prompt, match_.similarity
                ));
                diag = diag.with_fix_step("Update your input prompt to match the trace exactly");
            } else {
                diag = diag.with_fix_step("No similar prompts found in trace file");
            }

            diag = diag.with_fix_step("Regenerate the trace file: assay trace ingest ...");

            Err(anyhow::Error::new(diag))
        }
    }

    fn provider_name(&self) -> &'static str {
        "trace"
    }

    fn fingerprint(&self) -> Option<String> {
        Some(self.fingerprint.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_trace_client_happy_path() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt": "hello", "response": "world", "model": "gpt-4"}}"#
        )?;
        writeln!(tmp, r#"{{"prompt": "foo", "response": "bar"}}"#)?;

        let client = TraceClient::from_path(tmp.path())?;

        let resp1 = client.complete("hello", None).await?;
        assert_eq!(resp1.text, "world");
        assert_eq!(resp1.model, "gpt-4");

        let resp2 = client.complete("foo", None).await?;
        assert_eq!(resp2.text, "bar");
        assert_eq!(resp2.provider, "trace"); // default

        Ok(())
    }

    #[tokio::test]
    async fn test_trace_client_miss() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, r#"{{"prompt": "exists", "response": "yes"}}"#)?;

        let client = TraceClient::from_path(tmp.path())?;
        let result = client.complete("does not exist", None).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_trace_client_duplicate_prompt() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, r#"{{"prompt": "dup", "response": "1"}}"#)?;
        writeln!(tmp, r#"{{"prompt": "dup", "response": "2"}}"#)?;

        let result = TraceClient::from_path(tmp.path());
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_trace_client_duplicate_request_id() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // different prompts, same ID
        writeln!(
            tmp,
            r#"{{"request_id": "id1", "prompt": "p1", "response": "1"}}"#
        )?;
        writeln!(
            tmp,
            r#"{{"request_id": "id1", "prompt": "p2", "response": "2"}}"#
        )?;

        let result = TraceClient::from_path(tmp.path());
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Duplicate request_id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_trace_schema_validation() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Bad version (Legacy JSON with version but missing response should be skipped)
        writeln!(tmp, r#"{{"schema_version": 2, "prompt": "p"}}"#)?;
        let client = TraceClient::from_path(tmp.path())?;
        assert!(client.complete("p", None).await.is_err()); // Trace miss

        let mut tmp2 = NamedTempFile::new()?;
        // Bad type - should be ignored (Ok, empty) or Err depending on policy.
        // Current implementation ignores unknown types (forward compat).
        writeln!(
            tmp2,
            r#"{{"type": "wrong", "prompt": "p", "response": "r"}}"#
        )?;
        let client = TraceClient::from_path(tmp2.path())?;
        assert!(client.complete("p", None).await.is_err()); // "p" not found because line ignored

        let mut tmp3 = NamedTempFile::new()?;
        // Missing text/response
        writeln!(tmp3, r#"{{"prompt": "p"}}"#)?;
        // Valid legacy line but missing required response -> TraceClient skips it.
        // So client is empty, returns Ok.
        let client = TraceClient::from_path(tmp3.path())?;
        assert!(client.complete("p", None).await.is_err()); // Trace miss expected

        Ok(())
    }

    #[tokio::test]
    async fn test_trace_meta_preservation() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Using verbatim JSON from trace.jsonl (simplified)
        let json = r#"{"schema_version":1,"type":"assay.trace","request_id":"test-1","prompt":"Say hello","response":"Hello world","meta":{"assay":{"embeddings":{"model":"text-embedding-3-small","response":[0.1],"reference":[0.1]}}}}"#;
        writeln!(tmp, "{}", json)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("Say hello", None).await?;

        println!("Meta from test: {}", resp.meta);
        assert!(
            resp.meta.pointer("/assay/embeddings/response").is_some(),
            "Meta embeddings missing!"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_v2_replay_precedence() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Scenario: Input in Step Content should override nothing (it's first),
        // Output in 2nd Step should override 1st Step.

        let ep_start = r#"{"type":"episode_start","episode_id":"e1","timestamp":100,"input":null}"#;
        let step1 = r#"{"type":"step","episode_id":"e1","step_id":"s1","kind":"model","timestamp":101,"content":"{\"prompt\":\"original_prompt\",\"completion\":\"output_1\"}"}"#;
        // Step 2 has same prompt (ignored if input set) but new completion (should override)
        let step2 = r#"{"type":"step","episode_id":"e1","step_id":"s2","kind":"model","timestamp":102,"content":"{\"prompt\":\"ignored\",\"completion\":\"final_output\"}"}"#;
        // Step 3 has meta completion (should override content?) per our rule "last wins" for output
        let step3 = r#"{"type":"step","episode_id":"e1","step_id":"s3","kind":"model","timestamp":103,"content":null,"meta":{"gen_ai.completion":"meta_final"}}"#;

        let ep_end = r#"{"type":"episode_end","episode_id":"e1","timestamp":104}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;
        writeln!(tmp, "{}", step2)?;
        writeln!(tmp, "{}", step3)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("original_prompt", None).await?; // Should find via Step 1

        // Output should be from Step 3 (last one)
        assert_eq!(resp.text, "meta_final");

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_partial_episode() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // No episode_end
        let ep_start = r#"{"type":"episode_start","episode_id":"e_flush","timestamp":100,"input":{"prompt":"flush_me"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_flush","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"flushed_output\"}"}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("flush_me", None).await?;
        assert_eq!(resp.text, "flushed_output");

        Ok(())
    }

    #[tokio::test]
    async fn test_episode_end_with_null_meta_preserves_tool_calls() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_meta_null","timestamp":100,"input":{"prompt":"meta_null_prompt"},"meta":null}"#;
        let tool_call = r#"{"type":"tool_call","episode_id":"e_meta_null","step_id":"s1","timestamp":101,"tool_name":"fs.read","call_index":0,"args":{"path":"/tmp/demo.txt"}}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_meta_null","timestamp":102,"final_output":"done"}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", tool_call)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("meta_null_prompt", None).await?;
        assert_eq!(resp.text, "done");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_episode_end_propagates_step_model_to_response() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_model","timestamp":100,"input":{"prompt":"model_prompt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_model","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"model_output\",\"model\":\"gpt-4o-mini\"}"}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_model","timestamp":102}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("model_prompt", None).await?;
        assert_eq!(resp.text, "model_output");
        assert_eq!(resp.model, "gpt-4o-mini");

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_preserves_tool_calls_in_meta() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_eof_tools","timestamp":100,"input":{"prompt":"eof_tools_prompt"}}"#;
        let tool_call = r#"{"type":"tool_call","episode_id":"e_eof_tools","step_id":"s1","timestamp":101,"tool_name":"fs.write","call_index":0,"args":{"path":"/tmp/out.txt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_eof_tools","step_id":"s2","kind":"model","timestamp":102,"content":"{\"completion\":\"eof_output\"}"}"#;
        // Intentionally no episode_end: exercises EOF flush path.

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", tool_call)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("eof_tools_prompt", None).await?;
        assert_eq!(resp.text, "eof_output");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.write")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_from_path_invalid_json_has_line_context() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, "not-json")?;

        let err = match TraceClient::from_path(tmp.path()) {
            Ok(_) => panic!("invalid JSON must fail"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("Invalid trace format"));
        assert!(err.contains("line 1"));
        assert!(err.contains("Content: not-json"));

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_tool_fields_promote_to_tool_calls_meta() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt":"legacy_tool","response":"ok","tool":"fs.read","args":{{"path":"/tmp/demo.txt"}}}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("legacy_tool", None).await?;
        assert_eq!(resp.text, "ok");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/args/path")
                .and_then(|v| v.as_str()),
            Some("/tmp/demo.txt")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_preexisting_tool_calls_are_preserved_without_duplication(
    ) -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt":"legacy_with_calls","response":"ok","tool_calls":[{{"tool_name":"fs.read","args":{{"path":"/tmp/a"}}}},{{"tool_name":"fs.write","args":{{"path":"/tmp/b"}}}}]}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("legacy_with_calls", None).await?;
        assert_eq!(resp.text, "ok");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(2)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/1/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.write")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_tool_only_record_uses_ignore_fallback_prompt() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"tool":"fs.read","args":{{"path":"/tmp/input.txt"}},"result":"ok"}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("ignore", None).await?;
        assert_eq!(resp.text, "ok");

        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/args/path")
                .and_then(|v| v.as_str()),
            Some("/tmp/input.txt")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_v2_non_model_prompt_is_only_fallback() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start =
            r#"{"type":"episode_start","episode_id":"e_prio","timestamp":100,"input":null}"#;
        let step_tool = r#"{"type":"step","episode_id":"e_prio","step_id":"s_tool","kind":"tool","timestamp":101,"content":"{\"prompt\":\"fallback_prompt\",\"completion\":\"tool_out\"}","meta":{}}"#;
        let step_model = r#"{"type":"step","episode_id":"e_prio","step_id":"s_model","kind":"model","timestamp":102,"content":"{\"prompt\":\"authoritative_prompt\",\"completion\":\"model_out\"}","meta":{}}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_prio","timestamp":103}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step_tool)?;
        writeln!(tmp, "{}", step_model)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("authoritative_prompt", None).await?;
        assert_eq!(resp.text, "model_out");
        assert!(
            client.complete("fallback_prompt", None).await.is_err(),
            "fallback prompt must not remain addressable after model prompt extraction"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_duplicate_prompt_key_keeps_first_entry() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Duplicate key definition for TraceClient insertion is prompt string.
        // request_id differences do not allow overwriting an existing prompt key.
        writeln!(
            tmp,
            r#"{{"request_id":"r1","prompt":"dup_prompt","response":"first_response"}}"#
        )?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_dup","timestamp":100,"input":{"prompt":"dup_prompt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_dup","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"second_response\"}"}"#;
        // No episode_end on purpose; this exercises EOF flush path.
        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("dup_prompt", None).await?;
        assert_eq!(resp.text, "first_response");

        Ok(())
    }

    #[tokio::test]
    async fn test_from_path_accepts_crlf_jsonl_lines() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        use std::io::Write as _;
        tmp.as_file_mut().write_all(
            b"{\"prompt\":\"crlf_prompt_1\",\"response\":\"ok1\"}\r\n{\"prompt\":\"crlf_prompt_2\",\"response\":\"ok2\"}\r\n",
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp1 = client.complete("crlf_prompt_1", None).await?;
        let resp2 = client.complete("crlf_prompt_2", None).await?;
        assert_eq!(resp1.text, "ok1");
        assert_eq!(resp2.text, "ok2");

        Ok(())
    }
}
