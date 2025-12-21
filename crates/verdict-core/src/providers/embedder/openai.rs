use super::Embedder;
use async_trait::async_trait;
use serde_json::json;

pub struct OpenAIEmbedder {
    pub model: String,
    pub api_key: String,
    pub client: reqwest::Client,
}

impl OpenAIEmbedder {
    pub fn new(model: String, api_key: String) -> Self {
        Self {
            model,
            api_key,
            client: reqwest::Client::new(),
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
            "encoding_format": "float" // or base64 if we want to optimize later
        });

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

        let json: serde_json::Value = resp.json().await?;

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
