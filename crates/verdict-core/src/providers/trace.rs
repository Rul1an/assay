use crate::model::LlmResponse;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct TraceClient {
    // prompts -> response
    traces: Arc<HashMap<String, LlmResponse>>,
}

impl TraceClient {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| anyhow::anyhow!("failed to open trace file {}: {}", path.display(), e))?;
        let reader = std::io::BufReader::new(file);

        let mut traces = HashMap::new();
        use std::io::BufRead;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            // Expected schema: { "prompt": "...", "response": "..." ... }
            // Or maybe a more complex OTel structure?
            // For MVP simplicity, let's assume a schema compatible with our internal LlmResponse or a simple mapping.
            // Let's assume the JSONL contains objects that *can be deserialized* optionally, but primarily we need prompt + text.

            #[derive(serde::Deserialize)]
            struct TraceEntry {
                prompt: String,
                text: Option<String>,
                response: Option<String>, // alias
                // metadata
                #[serde(default)]
                meta: serde_json::Value,
                model: Option<String>,
                provider: Option<String>,
            }

            let entry: TraceEntry = serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("line {}: failed to parse trace: {}", i + 1, e))?;

            let text = entry.text.or(entry.response).unwrap_or_default();

            let resp = LlmResponse {
                text,
                provider: entry.provider.unwrap_or_else(|| "trace".into()),
                model: entry.model.unwrap_or_else(|| "trace_model".into()),
                cached: false,
                meta: entry.meta,
            };

            if traces.contains_key(&entry.prompt) {
                return Err(anyhow::anyhow!(
                    "Duplicate prompt found in trace file at line {}: {}",
                    i + 1,
                    entry.prompt
                ));
            }
            traces.insert(entry.prompt, resp);
        }

        Ok(Self {
            traces: Arc::new(traces),
        })
    }
}

#[async_trait]
impl LlmClient for TraceClient {
    async fn complete(
        &self,
        prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        if let Some(resp) = self.traces.get(prompt) {
            Ok(resp.clone())
        } else {
            // For now, fail if not found.
            // In a partial replay scenario, we might want to fallback, but Requirements say "Input adapters: simpele JSONL trace ingest"
            Err(anyhow::anyhow!(
                "Trace miss: prompt not found in loaded traces"
            ))
        }
    }

    fn provider_name(&self) -> &'static str {
        "trace"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_trace_client_happy_path() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt": "hello", "response": "world", "model": "gpt-4"}}"#
        )?;
        writeln!(tmp, r#"{{"prompt": "foo", "response": "bar"}}"#)?;

        let client = TraceClient::from_path(tmp.path())?;

        let resp1 = client.complete("hello", None).await?;
        assert_eq!(resp1.text, "world");
        assert_eq!(resp1.model, "gpt-4");

        let resp2 = client.complete("foo", None).await?;
        assert_eq!(resp2.text, "bar");
        assert_eq!(resp2.provider, "trace"); // default

        Ok(())
    }

    #[tokio::test]
    async fn test_trace_client_miss() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, r#"{{"prompt": "exists", "response": "yes"}}"#)?;

        let client = TraceClient::from_path(tmp.path())?;
        let result = client.complete("does not exist", None).await;
        assert!(result.is_err());
        Ok(())
    }
}
