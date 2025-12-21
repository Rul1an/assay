use crate::model::LlmResponse;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
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
        let mut request_ids = HashSet::new();
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
                schema_version: Option<u32>,
                r#type: Option<String>,
                request_id: Option<String>,
                prompt: String,
                // context, meta, model support
                text: Option<String>,
                response: Option<String>,
                #[serde(default)]
                meta: serde_json::Value,
                model: Option<String>,
                provider: Option<String>,
            }

            let entry: TraceEntry = serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("line {}: failed to parse trace: {}", i + 1, e))?;

            // Validate Schema
            if let Some(v) = entry.schema_version {
                if v != 1 {
                    return Err(anyhow::anyhow!(
                        "line {}: unsupported schema_version {}",
                        i + 1,
                        v
                    ));
                }
            }
            if let Some(t) = &entry.r#type {
                if t != "verdict.trace" {
                    return Err(anyhow::anyhow!("line {}: unsupported type {}", i + 1, t));
                }
            }

            let text = match entry.text.or(entry.response) {
                Some(t) => t,
                None => {
                    return Err(anyhow::anyhow!(
                        "line {}: missing `text`/`response` field",
                        i + 1
                    ))
                }
            };

            let resp = LlmResponse {
                text,
                provider: entry.provider.unwrap_or_else(|| "trace".into()),
                model: entry.model.unwrap_or_else(|| "trace_model".into()),
                cached: false,
                meta: entry.meta,
            };

            // Uniqueness Check
            if let Some(rid) = &entry.request_id {
                if request_ids.contains(rid) {
                    return Err(anyhow::anyhow!(
                        "line {}: Duplicate request_id {}",
                        i + 1,
                        rid
                    ));
                }
                request_ids.insert(rid.clone());
            }

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

    #[tokio::test]
    async fn test_trace_client_duplicate_prompt() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, r#"{{"prompt": "dup", "response": "1"}}"#)?;
        writeln!(tmp, r#"{{"prompt": "dup", "response": "2"}}"#)?;

        let result = TraceClient::from_path(tmp.path());
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_trace_client_duplicate_request_id() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // different prompts, same ID
        writeln!(
            tmp,
            r#"{{"request_id": "id1", "prompt": "p1", "response": "1"}}"#
        )?;
        writeln!(
            tmp,
            r#"{{"request_id": "id1", "prompt": "p2", "response": "2"}}"#
        )?;

        let result = TraceClient::from_path(tmp.path());
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Duplicate request_id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_trace_schema_validation() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Bad version
        writeln!(
            tmp,
            r#"{{"schema_version": 2, "prompt": "p", "response": "r"}}"#
        )?;
        assert!(TraceClient::from_path(tmp.path()).is_err());

        let mut tmp2 = NamedTempFile::new()?;
        // Bad type
        writeln!(
            tmp2,
            r#"{{"type": "wrong", "prompt": "p", "response": "r"}}"#
        )?;
        assert!(TraceClient::from_path(tmp2.path()).is_err());

        let mut tmp3 = NamedTempFile::new()?;
        // Missing text/response
        writeln!(tmp3, r#"{{"prompt": "p"}}"#)?;
        assert!(TraceClient::from_path(tmp3.path()).is_err());

        Ok(())
    }
}
