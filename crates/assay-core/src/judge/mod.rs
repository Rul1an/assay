mod judge_internal;
pub mod reliability;
use crate::model::TestInput;
use crate::providers::llm::LlmClient;
use crate::storage::judge_cache::JudgeCache;
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
    pub reliability: reliability::ReliabilityConfig,
    pub system_prompt_version: String,
}

pub(crate) struct JudgeCallResult {
    pub(crate) passed: bool,
    pub(crate) rationale: String,
}

#[derive(Clone)]
pub struct JudgeService {
    config: JudgeRuntimeConfig,
    cache: JudgeCache,
    client: Option<Arc<dyn LlmClient>>,
    pub(crate) global_extra_calls: Arc<std::sync::atomic::AtomicU32>,
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
            global_extra_calls: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn evaluate(
        &self,
        test_id: &str,
        rubric_id: &str,
        data: &TestInput,
        response_text: &str,
        suite_rubric_version: Option<&str>,
        meta: &mut serde_json::Value,
        seed: Option<u64>,
    ) -> anyhow::Result<()> {
        judge_internal::run::evaluate_impl(
            self,
            test_id,
            rubric_id,
            data,
            response_text,
            suite_rubric_version,
            meta,
            seed,
        )
        .await
    }
}
