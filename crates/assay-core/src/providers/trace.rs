use crate::errors::{diagnostic::codes, similarity::closest_prompt, Diagnostic};
use crate::model::LlmResponse;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use serde_json as sj;
use std::collections::HashMap;
use std::sync::Arc;

#[path = "trace_next/mod.rs"]
mod trace_next;

#[derive(Clone)]
pub struct TraceClient {
    traces: Arc<HashMap<String, LlmResponse>>,
    fingerprint: String,
}

impl TraceClient {
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        trace_next::from_path_impl(path)
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
            let closest = closest_prompt(prompt, self.traces.keys());

            let mut diag = Diagnostic::new(
                codes::E_TRACE_MISS,
                "Trace miss: prompt not found in loaded traces".to_string(),
            )
            .with_source("trace")
            .with_context(sj::json!({
                "prompt": prompt,
                "closest_match": closest
            }));

            if let Some(match_) = closest {
                diag = diag.with_fix_step(format!(
                    "Did you mean '{}'? (similarity: {:.2})",
                    match_.prompt, match_.similarity
                ));
                diag = diag.with_fix_step("Update your input prompt to match the trace exactly");
            } else {
                diag = diag.with_fix_step("No similar prompts found in trace file");
            }

            diag = diag.with_fix_step("Regenerate the trace file: assay trace ingest ...");

            Err(anyhow::Error::new(diag))
        }
    }

    fn provider_name(&self) -> &'static str {
        "trace"
    }

    fn fingerprint(&self) -> Option<String> {
        Some(self.fingerprint.clone())
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
        // Bad version (Legacy JSON with version but missing response should be skipped)
        writeln!(tmp, r#"{{"schema_version": 2, "prompt": "p"}}"#)?;
        let client = TraceClient::from_path(tmp.path())?;
        assert!(client.complete("p", None).await.is_err()); // Trace miss

        let mut tmp2 = NamedTempFile::new()?;
        // Bad type - should be ignored (Ok, empty) or Err depending on policy.
        // Current implementation ignores unknown types (forward compat).
        writeln!(
            tmp2,
            r#"{{"type": "wrong", "prompt": "p", "response": "r"}}"#
        )?;
        let client = TraceClient::from_path(tmp2.path())?;
        assert!(client.complete("p", None).await.is_err()); // "p" not found because line ignored

        let mut tmp3 = NamedTempFile::new()?;
        // Missing text/response
        writeln!(tmp3, r#"{{"prompt": "p"}}"#)?;
        // Valid legacy line but missing required response -> TraceClient skips it.
        // So client is empty, returns Ok.
        let client = TraceClient::from_path(tmp3.path())?;
        assert!(client.complete("p", None).await.is_err()); // Trace miss expected

        Ok(())
    }

    #[tokio::test]
    async fn test_trace_meta_preservation() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Using verbatim JSON from trace.jsonl (simplified)
        let json = r#"{"schema_version":1,"type":"assay.trace","request_id":"test-1","prompt":"Say hello","response":"Hello world","meta":{"assay":{"embeddings":{"model":"text-embedding-3-small","response":[0.1],"reference":[0.1]}}}}"#;
        writeln!(tmp, "{}", json)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("Say hello", None).await?;

        println!("Meta from test: {}", resp.meta);
        assert!(
            resp.meta.pointer("/assay/embeddings/response").is_some(),
            "Meta embeddings missing!"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_v2_replay_precedence() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Scenario: Input in Step Content should override nothing (it's first),
        // Output in 2nd Step should override 1st Step.

        let ep_start = r#"{"type":"episode_start","episode_id":"e1","timestamp":100,"input":null}"#;
        let step1 = r#"{"type":"step","episode_id":"e1","step_id":"s1","kind":"model","timestamp":101,"content":"{\"prompt\":\"original_prompt\",\"completion\":\"output_1\"}"}"#;
        // Step 2 has same prompt (ignored if input set) but new completion (should override)
        let step2 = r#"{"type":"step","episode_id":"e1","step_id":"s2","kind":"model","timestamp":102,"content":"{\"prompt\":\"ignored\",\"completion\":\"final_output\"}"}"#;
        // Step 3 has meta completion (should override content?) per our rule "last wins" for output
        let step3 = r#"{"type":"step","episode_id":"e1","step_id":"s3","kind":"model","timestamp":103,"content":null,"meta":{"gen_ai.completion":"meta_final"}}"#;

        let ep_end = r#"{"type":"episode_end","episode_id":"e1","timestamp":104}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;
        writeln!(tmp, "{}", step2)?;
        writeln!(tmp, "{}", step3)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("original_prompt", None).await?; // Should find via Step 1

        // Output should be from Step 3 (last one)
        assert_eq!(resp.text, "meta_final");

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_partial_episode() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // No episode_end
        let ep_start = r#"{"type":"episode_start","episode_id":"e_flush","timestamp":100,"input":{"prompt":"flush_me"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_flush","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"flushed_output\"}"}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("flush_me", None).await?;
        assert_eq!(resp.text, "flushed_output");

        Ok(())
    }

    #[tokio::test]
    async fn test_episode_end_with_null_meta_preserves_tool_calls() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_meta_null","timestamp":100,"input":{"prompt":"meta_null_prompt"},"meta":null}"#;
        let tool_call = r#"{"type":"tool_call","episode_id":"e_meta_null","step_id":"s1","timestamp":101,"tool_name":"fs.read","call_index":0,"args":{"path":"/tmp/demo.txt"}}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_meta_null","timestamp":102,"final_output":"done"}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", tool_call)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("meta_null_prompt", None).await?;
        assert_eq!(resp.text, "done");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_episode_end_propagates_step_model_to_response() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_model","timestamp":100,"input":{"prompt":"model_prompt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_model","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"model_output\",\"model\":\"gpt-4o-mini\"}"}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_model","timestamp":102}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("model_prompt", None).await?;
        assert_eq!(resp.text, "model_output");
        assert_eq!(resp.model, "gpt-4o-mini");

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_preserves_tool_calls_in_meta() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_eof_tools","timestamp":100,"input":{"prompt":"eof_tools_prompt"}}"#;
        let tool_call = r#"{"type":"tool_call","episode_id":"e_eof_tools","step_id":"s1","timestamp":101,"tool_name":"fs.write","call_index":0,"args":{"path":"/tmp/out.txt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_eof_tools","step_id":"s2","kind":"model","timestamp":102,"content":"{\"completion\":\"eof_output\"}"}"#;
        // Intentionally no episode_end: exercises EOF flush path.

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", tool_call)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("eof_tools_prompt", None).await?;
        assert_eq!(resp.text, "eof_output");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.write")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_from_path_invalid_json_has_line_context() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(tmp, "not-json")?;

        let err = match TraceClient::from_path(tmp.path()) {
            Ok(_) => panic!("invalid JSON must fail"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("Invalid trace format"));
        assert!(err.contains("line 1"));
        assert!(err.contains("Content: not-json"));

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_tool_fields_promote_to_tool_calls_meta() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt":"legacy_tool","response":"ok","tool":"fs.read","args":{{"path":"/tmp/demo.txt"}}}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("legacy_tool", None).await?;
        assert_eq!(resp.text, "ok");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/args/path")
                .and_then(|v| v.as_str()),
            Some("/tmp/demo.txt")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_preexisting_tool_calls_are_preserved_without_duplication(
    ) -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"prompt":"legacy_with_calls","response":"ok","tool_calls":[{{"tool_name":"fs.read","args":{{"path":"/tmp/a"}}}},{{"tool_name":"fs.write","args":{{"path":"/tmp/b"}}}}]}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("legacy_with_calls", None).await?;
        assert_eq!(resp.text, "ok");
        assert_eq!(
            resp.meta
                .pointer("/tool_calls")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(2)
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/1/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.write")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_legacy_tool_only_record_uses_ignore_fallback_prompt() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{"tool":"fs.read","args":{{"path":"/tmp/input.txt"}},"result":"ok"}}"#
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("ignore", None).await?;
        assert_eq!(resp.text, "ok");

        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/tool_name")
                .and_then(|v| v.as_str()),
            Some("fs.read")
        );
        assert_eq!(
            resp.meta
                .pointer("/tool_calls/0/args/path")
                .and_then(|v| v.as_str()),
            Some("/tmp/input.txt")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_v2_non_model_prompt_is_only_fallback() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        let ep_start =
            r#"{"type":"episode_start","episode_id":"e_prio","timestamp":100,"input":null}"#;
        let step_tool = r#"{"type":"step","episode_id":"e_prio","step_id":"s_tool","kind":"tool","timestamp":101,"content":"{\"prompt\":\"fallback_prompt\",\"completion\":\"tool_out\"}","meta":{}}"#;
        let step_model = r#"{"type":"step","episode_id":"e_prio","step_id":"s_model","kind":"model","timestamp":102,"content":"{\"prompt\":\"authoritative_prompt\",\"completion\":\"model_out\"}","meta":{}}"#;
        let ep_end = r#"{"type":"episode_end","episode_id":"e_prio","timestamp":103}"#;

        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step_tool)?;
        writeln!(tmp, "{}", step_model)?;
        writeln!(tmp, "{}", ep_end)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("authoritative_prompt", None).await?;
        assert_eq!(resp.text, "model_out");
        assert!(
            client.complete("fallback_prompt", None).await.is_err(),
            "fallback prompt must not remain addressable after model prompt extraction"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_eof_flush_duplicate_prompt_key_keeps_first_entry() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        // Duplicate key definition for TraceClient insertion is prompt string.
        // request_id differences do not allow overwriting an existing prompt key.
        writeln!(
            tmp,
            r#"{{"request_id":"r1","prompt":"dup_prompt","response":"first_response"}}"#
        )?;
        let ep_start = r#"{"type":"episode_start","episode_id":"e_dup","timestamp":100,"input":{"prompt":"dup_prompt"}}"#;
        let step1 = r#"{"type":"step","episode_id":"e_dup","step_id":"s1","kind":"model","timestamp":101,"content":"{\"completion\":\"second_response\"}"}"#;
        // No episode_end on purpose; this exercises EOF flush path.
        writeln!(tmp, "{}", ep_start)?;
        writeln!(tmp, "{}", step1)?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp = client.complete("dup_prompt", None).await?;
        assert_eq!(resp.text, "first_response");

        Ok(())
    }

    #[tokio::test]
    async fn test_from_path_accepts_crlf_jsonl_lines() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        use std::io::Write as _;
        tmp.as_file_mut().write_all(
            b"{\"prompt\":\"crlf_prompt_1\",\"response\":\"ok1\"}\r\n{\"prompt\":\"crlf_prompt_2\",\"response\":\"ok2\"}\r\n",
        )?;

        let client = TraceClient::from_path(tmp.path())?;
        let resp1 = client.complete("crlf_prompt_1", None).await?;
        let resp2 = client.complete("crlf_prompt_2", None).await?;
        assert_eq!(resp1.text, "ok1");
        assert_eq!(resp2.text, "ok2");

        Ok(())
    }
}
