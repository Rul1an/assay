use super::LlmClient;
use crate::model::LlmResponse;
use crate::vcr::{VcrClient, VcrMode};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct OpenAIClient {
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub client: reqwest::Client,
    /// Optional VCR client for record/replay (shared, requires mutex for async)
    vcr: Option<Arc<Mutex<VcrClient>>>,
}

impl OpenAIClient {
    pub fn new(model: String, api_key: String, temperature: f32, max_tokens: u32) -> Self {
        Self {
            model,
            api_key,
            temperature,
            max_tokens,
            client: reqwest::Client::new(),
            vcr: None,
        }
    }

    /// Create with VCR support (record/replay HTTP responses)
    pub fn with_vcr(
        model: String,
        api_key: String,
        temperature: f32,
        max_tokens: u32,
        vcr: Arc<Mutex<VcrClient>>,
    ) -> Self {
        Self {
            model,
            api_key,
            temperature,
            max_tokens,
            client: reqwest::Client::new(),
            vcr: Some(vcr),
        }
    }

    /// Create from environment (auto-enables VCR if ASSAY_VCR_MODE is set)
    pub fn from_env(model: String, api_key: String, temperature: f32, max_tokens: u32) -> Self {
        let vcr_mode = VcrMode::from_env();
        if vcr_mode != VcrMode::Off {
            let vcr = VcrClient::from_env();
            Self::with_vcr(
                model,
                api_key,
                temperature,
                max_tokens,
                Arc::new(Mutex::new(vcr)),
            )
        } else {
            Self::new(model, api_key, temperature, max_tokens)
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

        let json: serde_json::Value = if let Some(vcr) = &self.vcr {
            // Use VCR for record/replay
            let mut vcr_guard = vcr.lock().await;
            let auth = format!("Bearer {}", self.api_key);
            let resp = vcr_guard.post_json(url, &body, Some(&auth)).await?;

            if !resp.is_success() {
                anyhow::bail!(
                    "OpenAI chat API error (status {}): {}",
                    resp.status,
                    resp.body
                );
            }
            resp.body
        } else {
            // Direct HTTP request
            crate::providers::network::check_outbound(url)?;
            let resp = self
                .client
                .post(url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_else(|_| String::new());
                anyhow::bail!("OpenAI chat API error: {}", error_text);
            }

            resp.json().await?
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn openai_client_respects_network_deny_policy() {
        let _serial = crate::providers::network::lock_test_serial_async().await;
        let _guard = crate::providers::network::NetworkPolicyGuard::deny("unit test");
        let client = OpenAIClient::new("gpt-4o-mini".to_string(), "test-key".to_string(), 0.0, 8);
        let err = client
            .complete("hello", None)
            .await
            .expect_err("network deny policy should block outbound call");
        let msg = err.to_string();
        assert!(msg.contains("outbound network blocked by policy"));
        assert!(msg.contains("api.openai.com"));
    }
}
