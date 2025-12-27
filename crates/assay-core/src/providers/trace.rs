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

        // State for accumulating V2 episodes
        struct EpisodeState {
            input: Option<String>,
            output: Option<String>,
            model: Option<String>,
            meta: serde_json::Value,
            input_is_model: bool,
            tool_calls: Vec<crate::model::ToolCallRecord>,
        }
        let mut active_episodes: HashMap<String, EpisodeState> = HashMap::new();

        for (i, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            if line.trim().is_empty() {
                continue;
            }

            // Attempt V2 Parse first (TraceEntry enum)
            // If it fails, fallback to legacy V1 (TraceEntryV1/TraceEntry struct local def)
            // Actually, we can use `TraceEntry` enum from schema if we have it?
            // But schema might not be strictly followed in loose JSON files.
            // Let's use serde_json::Value to sniff.

            let v: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("line {}: parse error: {}", i + 1, e))?;

            // Heuristic detection
            let mut prompt_opt = None;
            let mut response_opt = None;
            let mut model = "trace".to_string();
            let mut meta = serde_json::json!({});
            let mut request_id_check = None;

            if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                match t {
                    "assay.trace" => {
                        // V1
                        prompt_opt = v.get("prompt").and_then(|s| s.as_str()).map(String::from);
                        response_opt = v
                            .get("response")
                            .or(v.get("text"))
                            .and_then(|s| s.as_str())
                            .map(String::from);
                        if let Some(m) = v.get("model").and_then(|s| s.as_str()) {
                            model = m.to_string();
                        }
                        if let Some(m) = v.get("meta") {
                            meta = m.clone();
                        }
                        if let Some(r) = v.get("request_id").and_then(|s| s.as_str()) {
                            request_id_check = Some(r.to_string());
                        }
                    }
                    "episode_start" => {
                        // START V2
                        if let Ok(ev) =
                            serde_json::from_value::<crate::trace::schema::EpisodeStart>(v.clone())
                        {
                            let input_prompt = ev
                                .input
                                .get("prompt")
                                .and_then(|s| s.as_str())
                                .map(String::from);
                            let has_input = input_prompt.is_some();
                            let state = EpisodeState {
                                input: input_prompt,
                                output: None, // accum later
                                model: None,  // extract from steps?
                                meta: ev.meta,
                                input_is_model: has_input, // authoritative only if present
                                tool_calls: Vec::new(),
                            };
                            active_episodes.insert(ev.episode_id, state);
                            continue; // Wait for end
                        }
                    }
                    "tool_call" => {
                        if let Ok(ev) =
                            serde_json::from_value::<crate::trace::schema::ToolCallEntry>(v.clone())
                        {
                            if let Some(state) = active_episodes.get_mut(&ev.episode_id) {
                                state.tool_calls.push(crate::model::ToolCallRecord {
                                    id: format!("{}-{}", ev.step_id, ev.call_index.unwrap_or(0)),
                                    tool_name: ev.tool_name,
                                    args: ev.args,
                                    result: ev.result,
                                    error: ev.error.map(serde_json::Value::String),
                                    index: state.tool_calls.len(), // Global index for sequence validation
                                    ts_ms: ev.timestamp,
                                });
                            }
                        }
                    }
                    "episode_end" => {
                        // END V2
                        if let Ok(ev) =
                            serde_json::from_value::<crate::trace::schema::EpisodeEnd>(v.clone())
                        {
                            if let Some(mut state) = active_episodes.remove(&ev.episode_id) {
                                // Finalize
                                if let Some(out) = ev.final_output {
                                    state.output = Some(out);
                                }

                                if let Some(p) = state.input {
                                    prompt_opt = Some(p);
                                    response_opt = state.output;

                                    // Inject tool calls into meta
                                    if !state.tool_calls.is_empty() {
                                        state.meta["tool_calls"] =
                                            serde_json::to_value(&state.tool_calls)
                                                .unwrap_or_default();
                                    }

                                    meta = state.meta;
                                    // model?
                                }
                            }
                        }
                    }

                    "step" => {
                        if let Ok(ev) =
                            serde_json::from_value::<crate::trace::schema::StepEntry>(v.clone())
                        {
                            if let Some(state) = active_episodes.get_mut(&ev.episode_id) {
                                // PROMPT EXTRACTION
                                // Logic:
                                // 1. If step is MODEL: Prefer this prompt over any previous (unless locked? No, "First Wins" for model steps).
                                //    Actually standard "First Wins" means first MODEL step.
                                // 2. If step is NOT model: Use as fallback only if we have NO input yet.

                                let is_model = ev.kind == "model";
                                let can_extract = if is_model {
                                    // If we are model, we overwrite if current input is NOT model (fallback) OR if input is None.
                                    // If we already have a model input, we skip (First Model Wins).
                                    !state.input_is_model
                                } else {
                                    // If not model, only extract if we have absolutely nothing.
                                    state.input.is_none()
                                };

                                if can_extract {
                                    let mut found_prompt = None;

                                    if let Some(c) = &ev.content {
                                        if let Ok(c_json) =
                                            serde_json::from_str::<serde_json::Value>(c)
                                        {
                                            if let Some(p) =
                                                c_json.get("prompt").and_then(|s| s.as_str())
                                            {
                                                found_prompt = Some(p.to_string());
                                            }
                                        }
                                    }
                                    if found_prompt.is_none() {
                                        if let Some(p) =
                                            ev.meta.get("gen_ai.prompt").and_then(|s| s.as_str())
                                        {
                                            found_prompt = Some(p.to_string());
                                        }
                                    }

                                    if let Some(p) = found_prompt {
                                        state.input = Some(p);
                                        if is_model {
                                            state.input_is_model = true;
                                        }
                                        // DEBUG: remove me
                                        /*
                                        eprintln!("DEBUG: TraceClient extracted prompt: '{}' is_model={}", state.input.as_ref().unwrap(), is_model);
                                        */
                                    }
                                }

                                // --- OUTPUT EXTRACTION (Last Wins) ---
                                // Rule 4: Step Content "completion"
                                if let Some(c) = &ev.content {
                                    let mut extracted = None;
                                    if let Ok(c_json) = serde_json::from_str::<serde_json::Value>(c)
                                    {
                                        if let Some(resp) =
                                            c_json.get("completion").and_then(|s| s.as_str())
                                        {
                                            extracted = Some(resp.to_string());
                                            // Capture model if present
                                            if let Some(m) =
                                                c_json.get("model").and_then(|s| s.as_str())
                                            {
                                                state.model = Some(m.to_string());
                                            }
                                        }
                                    }

                                    if let Some(out) = extracted {
                                        state.output = Some(out);
                                    } else {
                                        // Fallback: use raw content as output if structured extraction failed
                                        state.output = Some(c.clone());
                                    }
                                }
                                // Rule 5: Step Meta "gen_ai.completion"
                                if let Some(resp) =
                                    ev.meta.get("gen_ai.completion").and_then(|s| s.as_str())
                                {
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
                        continue;
                    }
                    _ => {
                        continue;
                    }
                }
            } else {
                // Legacy loose JSON (no type)
                prompt_opt = v.get("prompt").and_then(|s| s.as_str()).map(String::from);
                response_opt = v
                    .get("response")
                    .or(v.get("text"))
                    .and_then(|s| s.as_str())
                    .map(String::from);
                // Fix: Extract other fields too
                if let Some(m) = v.get("model").and_then(|s| s.as_str()) {
                    model = m.to_string();
                }
                if let Some(m) = v.get("meta") {
                    meta = m.clone();
                }
                if let Some(r) = v.get("request_id").and_then(|s| s.as_str()) {
                    request_id_check = Some(r.to_string());
                }
            }

            if let (Some(p), Some(r)) = (prompt_opt, response_opt) {
                // Finalize Entry
                // Uniqueness Check
                if let Some(rid) = &request_id_check {
                    if request_ids.contains(rid) {
                        return Err(anyhow::anyhow!(
                            "line {}: Duplicate request_id {}",
                            i + 1,
                            rid
                        ));
                    }
                    request_ids.insert(rid.clone());
                }

                if traces.contains_key(&p) {
                    // Duplicate prompt handling? Overwrite or Error?
                    // Existing code errors.
                    return Err(anyhow::anyhow!(
                        "Duplicate prompt found in trace file: {}",
                        p
                    ));
                }

                traces.insert(
                    p,
                    LlmResponse {
                        text: r,
                        meta,
                        model,
                        provider: "trace".to_string(),
                        ..Default::default()
                    },
                );
            }
        }

        // Flush active episodes at EOF
        for (id, state) in active_episodes {
            if let (Some(p), Some(r)) = (state.input.clone(), state.output.clone()) {
                // ... reuse insertion logic (refactor to helper?) ...
                // Duplicate check
                if traces.contains_key(&p) {
                    eprintln!("Warning: Duplicate prompt skipped at EOF for id {}", id);
                    continue;
                }
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

        // Compute deterministic fingerprint of traces
        let mut keys: Vec<&String> = traces.keys().collect();
        keys.sort();
        let mut hasher = sha2::Sha256::new();
        for k in keys {
            use sha2::Digest;
            hasher.update(k.as_bytes());
            if let Some(v) = traces.get(k) {
                // hash validation relevant parts of response
                hasher.update(v.text.as_bytes());
                // include meta/model? yes for completeness
                hasher.update(v.model.as_bytes());
            }
        }
        let fingerprint = hex::encode(hasher.finalize());

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
}
