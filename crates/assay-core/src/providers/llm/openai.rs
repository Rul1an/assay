use super::LlmClient;
use crate::model::LlmResponse;
use async_trait::async_trait;
use serde_json::json;

pub struct OpenAIClient {
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub client: reqwest::Client,
}

impl OpenAIClient {
    pub fn new(model: String, api_key: String, temperature: f32, max_tokens: u32) -> Self {
        Self {
            model,
            api_key,
            temperature,
            max_tokens,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn complete(
        &self,
        prompt: &str,
        context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        let url = "https://api.openai.com/v1/chat/completions";

        let mut messages = Vec::new();

        // Construct message
        // If context provided, try to incorporate it.
        // Simple strategy:
        // User: [Context] ... [Prompt]
        let content = if let Some(ctx) = context {
            format!("Context:\n{:?}\n\nQuestion: {}", ctx, prompt)
        } else {
            prompt.to_string()
        };

        messages.push(json!({
            "role": "user",
            "content": content
        }));

        let body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
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
            anyhow::bail!("OpenAI chat API error: {}", error_text);
        }

        let json: serde_json::Value = resp.json().await?;

        // Parse choices[0].message.content
        let text = json
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("OpenAI API response missing content"))?
            .to_string();

        Ok(LlmResponse {
            text,
            provider: "openai".to_string(),
            model: self.model.clone(),
            cached: false,
            meta: json!({}),
        })
    }

    fn provider_name(&self) -> &'static str {
        "openai"
    }
}
