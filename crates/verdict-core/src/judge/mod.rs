use crate::model::TestInput;
use crate::providers::llm::LlmClient;
use crate::storage::judge_cache::JudgeCache;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct JudgeRuntimeConfig {
    pub enabled: bool,
    pub provider: String, // "openai", "fake", "none"
    pub model: Option<String>,
    pub samples: u32,
    pub temperature: f32,
    pub max_tokens: u32,
    pub refresh: bool,
}

#[derive(Clone)]
pub struct JudgeService {
    config: JudgeRuntimeConfig,
    cache: JudgeCache,
    client: Option<Arc<dyn LlmClient>>,
}

impl JudgeService {
    pub fn new(
        config: JudgeRuntimeConfig,
        cache: JudgeCache,
        client: Option<Arc<dyn LlmClient>>,
    ) -> Self {
        Self {
            config,
            cache,
            client,
        }
    }

    pub async fn evaluate(
        &self,
        test_id: &str,
        rubric_id: &str,
        data: &TestInput,
        response_text: &str,
        suite_rubric_version: Option<&str>,
        meta: &mut serde_json::Value,
    ) -> anyhow::Result<()> {
        let rubric_version = suite_rubric_version.unwrap_or("v1");

        // 1. Trace Check
        if let Some(_trace_judge) = meta.pointer(&format!("/verdict/judge/{}", rubric_id)) {
            // Already present in trace
            // We could validate it, but for now accept it.
            // Ensure "source" is "trace" if not set?
            return Ok(());
        }

        // 2. Judge Disabled Check
        if !self.config.enabled {
            anyhow::bail!(
                "config error: test '{}' requires judge results ('{}:{}'), but judge is disabled.\n\
                 hint: options:\n\
                 1) run live judge: verdict ci --judge openai\n\
                 2) run replay/CI offline: provide trace meta at meta.verdict.judge.{}\n\
                 and re-run with: verdict ci --trace-file traces.jsonl --no-judge",
                test_id, rubric_id, rubric_version, rubric_id
            );
        }

        let client = self.client.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "config error: judge enabled but no client provided (verify --judge <provider>)"
            )
        })?;

        // 3. Cache Check
        let prompt = format!(
            "Rubric: {}\nInput: {}\nResponse: {}\nContext: {:?}",
            rubric_id, data.prompt, response_text, data.context
        );
        let input_hash = format!("{:x}", md5::compute(&prompt)); // Simple hash
        let cache_key = self.generate_cache_key(rubric_id, rubric_version, &input_hash);

        if !self.config.refresh {
            if let Some(mut cached) = self.cache.get(&cache_key)? {
                if let Some(obj) = cached.as_object_mut() {
                    obj.insert("source".to_string(), json!("cache"));
                    obj.insert(
                        "cached_at".to_string(),
                        json!(chrono::Utc::now().to_rfc3339()),
                    );
                }
                self.inject_result(meta, rubric_id, cached)?;
                return Ok(());
            }
        }

        // 4. Live Call (Voting)
        let samples = self.config.samples;
        let mut votes = Vec::new();
        let mut rationales = Vec::new();

        for _ in 0..samples {
            // In a real impl, we'd use the actual rubric prompt template
            let _sys_prompt = format!("You are a judge for rubric {}. Output JSON with {{passed: bool, rationale: string}}.", rubric_id);
            // This prompt is simplistic; strict impl would use templates.
            let resp = client.complete(&prompt, None).await?; // Assuming prompt contains everything
                                                              // Parse JSON
                                                              // Mock parsing for now if fake/dummy, or try parse
                                                              // For MVP, if client is dummy, it returns text.
                                                              // We need to robustly parse the LLM output.

            // Assume the client returns a string that contains JSON.
            // If dummy: "hello from dummy". This won't parse.
            // If "fake" embedder logic was here? No, client is LlmClient.

            // For now, let's assume the LLM returns valid JSON or we fail.
            // We need a proper rubric prompt construction.
            votes.push(self.mock_vote_logic(rubric_id, &resp.text)); // Temp mock
            rationales.push(resp.text);
        }

        // Aggregation
        let pass_count = votes.iter().filter(|&&v| v).count() as u32;
        let agreement = pass_count as f64 / samples as f64;
        let passed = pass_count as f64 > (samples as f64 / 2.0); // Majority

        // Status check
        // If disagreement (agreement < 1.0), we might warn later in the Metric logic?
        // Or store "unstable": true in meta?

        let result = json!({
            "rubric_version": rubric_version,
            "passed": passed,
            "score": agreement, // Score is agreement ratio? Or binary?
            // Usually score is 1.0 (pass) or 0.0 (fail) or agreement?
            "source": "live",
            "samples": votes,
            "agreement": agreement,
            "rationale": rationales.first().cloned().unwrap_or_default(), // Take first
            "cached_at": chrono::Utc::now().to_rfc3339()
        });

        // Store in Cache
        self.cache.put(
            &cache_key,
            &self.config.provider,
            self.config.model.as_deref().unwrap_or("default"),
            rubric_id,
            rubric_version,
            &result,
        )?;

        self.inject_result(meta, rubric_id, result)?;

        Ok(())
    }

    fn generate_cache_key(
        &self,
        rubric_id: &str,
        rubric_version: &str,
        input_hash: &str,
    ) -> String {
        // TODO: use actual template hash
        let template_version = "v1-simple";
        let raw = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.config.provider,
            self.config.model.as_deref().unwrap_or(""),
            rubric_id,
            rubric_version,
            self.config.temperature,
            self.config.max_tokens,
            self.config.samples,
            template_version,
            input_hash
        );
        format!("{:x}", md5::compute(raw))
    }

    fn inject_result(
        &self,
        meta: &mut serde_json::Value,
        rubric_id: &str,
        result: serde_json::Value,
    ) -> anyhow::Result<()> {
        if let Some(obj) = meta.as_object_mut() {
            let verdict = obj
                .entry("verdict")
                .or_insert(json!({}))
                .as_object_mut()
                .unwrap();
            let judge = verdict
                .entry("judge")
                .or_insert(json!({}))
                .as_object_mut()
                .unwrap();
            judge.insert(rubric_id.to_string(), result);
        }
        Ok(())
    }

    // logic to "mock" vote if text isn't JSON (for dev speed)
    fn mock_vote_logic(&self, _rubric: &str, text: &str) -> bool {
        // If "dummy" client, always pass?
        // Or check text content
        !text.contains("fail")
    }
}
