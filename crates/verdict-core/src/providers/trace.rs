use crate::model::LlmResponse;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct TraceClient {
    // prompts -> response
    traces: Arc<HashMap<String, LlmResponse>>,
}

impl TraceClient {
    pub fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let file = File::open(path.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "failed to open trace file '{}': {}",
                path.as_ref().display(),
                e
            )
        })?;
        let reader = std::io::BufReader::new(file);

        let mut traces = HashMap::new();
        let mut request_ids = HashSet::new();

        for (i, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            if line.trim().is_empty() {
                continue;
            }

            // Expected schema: { "prompt": "...", "response": "..." ... }

            // Expected schema: { "prompt": "...", "response": "..." ... }
            // Or maybe a more complex OTel structure?
            // For MVP simplicity, let's assume a schema compatible with our internal LlmResponse or a simple mapping.
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
                    continue; // Skip unknown versions or error? For now skip
                }
            }
            if let Some(t) = &entry.r#type {
                if t != "verdict.trace" {
                    continue;
                }
            }

            let response_text = entry.response.or(entry.text).unwrap_or_default();

            // Construct LlmResponse
            let resp = LlmResponse {
                text: response_text,
                meta: entry.meta,
                model: entry.model.unwrap_or_else(|| "trace".to_string()),
                provider: entry.provider.unwrap_or_else(|| "trace".to_string()),
                ..Default::default()
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

    #[tokio::test]
    async fn test_trace_meta_preservation() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Using verbatim JSON from trace.jsonl (simplified)
        let json = r#"{"schema_version":1,"type":"verdict.trace","request_id":"test-1","prompt":"Say hello","response":"Hello world","meta":{"verdict":{"embeddings":{"model":"text-embedding-3-small","response":[0.1],"reference":[0.1]}}}}"#;
        writeln!(tmp, "{}", json)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("Say hello", None).await?;

        println!("Meta from test: {}", resp.meta);
        assert!(
            resp.meta.pointer("/verdict/embeddings/response").is_some(),
            "Meta embeddings missing!"
        );
        Ok(())
    }
}
