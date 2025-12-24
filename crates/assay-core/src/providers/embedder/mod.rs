use async_trait::async_trait;

pub mod fake;
pub mod openai;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn name(&self) -> &'static str;
    fn model_id(&self) -> String;
}
