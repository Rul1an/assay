use super::LlmClient;
use crate::model::LlmResponse;
use async_trait::async_trait;

#[derive(Debug)]
pub struct FakeClient {
    model: String,
    fixed_response: Option<String>,
}

impl FakeClient {
    pub fn new(model: String) -> Self {
        Self {
            model,
            fixed_response: None,
        }
    }

    pub fn with_response(mut self, response: String) -> Self {
        self.fixed_response = Some(response);
        self
    }
}

#[async_trait]
impl LlmClient for FakeClient {
    async fn complete(
        &self,
        _prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        let text = self.fixed_response.clone().unwrap_or_else(|| {
            // Default behavior: echo reliable "passed" for judge tests
            // or just echo
            "passed".to_string()
        });

        Ok(LlmResponse {
            text,
            provider: "fake".to_string(),
            model: self.model.clone(),
            cached: false,
            meta: serde_json::json!({}),
        })
    }

    fn provider_name(&self) -> &'static str {
        "fake"
    }
}
