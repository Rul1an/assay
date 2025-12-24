use crate::model::LlmResponse;
use crate::providers::embedder::Embedder;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use std::sync::Arc;

// Strict LlmClient
pub struct StrictLlmClient {
    inner: Arc<dyn LlmClient>,
}

impl StrictLlmClient {
    pub fn new(inner: Arc<dyn LlmClient>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl LlmClient for StrictLlmClient {
    async fn complete(
        &self,
        _prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        anyhow::bail!("config error: --replay-strict forbids live LLM calls. Provide --trace-file or precomputed trace entries.");
    }

    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }
}

// Strict Embedder
pub struct StrictEmbedder {
    inner: Arc<dyn Embedder>,
}

impl StrictEmbedder {
    pub fn new(inner: Arc<dyn Embedder>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Embedder for StrictEmbedder {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("config error: --replay-strict forbids live Embedder calls. Ensure precomputed embeddings are present in trace.");
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn model_id(&self) -> String {
        self.inner.model_id()
    }
}
