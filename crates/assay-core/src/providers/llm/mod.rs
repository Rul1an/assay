use crate::model::LlmResponse;
use async_trait::async_trait;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(
        &self,
        prompt: &str,
        context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse>;
    fn provider_name(&self) -> &'static str;
    fn fingerprint(&self) -> Option<String> {
        None
    }
}

pub mod fake;
pub mod openai;
