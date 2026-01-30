use super::Embedder;
use crate::vcr::{VcrClient, VcrMode};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct OpenAIEmbedder {
    pub model: String,
    pub api_key: String,
    pub client: reqwest::Client,
    /// Optional VCR client for record/replay (shared, requires mutex for async)
    vcr: Option<Arc<Mutex<VcrClient>>>,
}

impl OpenAIEmbedder {
    pub fn new(model: String, api_key: String) -> Self {
        Self {
            model,
            api_key,
            client: reqwest::Client::new(),
            vcr: None,
        }
    }

    /// Create with VCR support (record/replay HTTP responses)
    pub fn with_vcr(model: String, api_key: String, vcr: Arc<Mutex<VcrClient>>) -> Self {
        Self {
            model,
            api_key,
            client: reqwest::Client::new(),
            vcr: Some(vcr),
        }
    }

    /// Create from environment (auto-enables VCR if ASSAY_VCR_MODE is set)
    pub fn from_env(model: String, api_key: String) -> Self {
        let vcr_mode = VcrMode::from_env();
        if vcr_mode != VcrMode::Off {
            let vcr = VcrClient::from_env();
            Self::with_vcr(model, api_key, Arc::new(Mutex::new(vcr)))
        } else {
            Self::new(model, api_key)
        }
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = "https://api.openai.com/v1/embeddings";
        let body = json!({
            "input": text,
            "model": self.model,
            "encoding_format": "float"
        });

        let json: serde_json::Value = if let Some(vcr) = &self.vcr {
            // Use VCR for record/replay
            let mut vcr_guard = vcr.lock().await;
            let auth = format!("Bearer {}", self.api_key);
            let resp = vcr_guard.post_json(url, &body, Some(&auth)).await?;

            if !resp.is_success() {
                anyhow::bail!(
                    "OpenAI embeddings API error (status {}): {}",
                    resp.status,
                    resp.body
                );
            }
            resp.body
        } else {
            // Direct HTTP request
            let resp = self
                .client
                .post(url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI embeddings API error: {}", error_text);
            }

            resp.json().await?
        };

        // Parse data[0].embedding
        let vec = json
            .pointer("/data/0/embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("OpenAI API response missing embedding field"))?;

        let floats: Vec<f32> = vec
            .iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect();

        Ok(floats)
    }

    fn name(&self) -> &'static str {
        "openai"
    }

    fn model_id(&self) -> String {
        self.model.clone()
    }
}
