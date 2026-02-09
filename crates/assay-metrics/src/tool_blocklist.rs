use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;

use crate::tool_calls::extract_tool_calls_canonical_or_empty;

pub struct ToolBlocklistMetric;

#[async_trait]
impl Metric for ToolBlocklistMetric {
    fn name(&self) -> &'static str {
        "tool_blocklist"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let blocked = match expected {
            Expected::ToolBlocklist { blocked } => blocked,
            _ => return Ok(MetricResult::pass(1.0)), // N/A
        };

        let tool_calls: Vec<ToolCallRecord> = extract_tool_calls_canonical_or_empty(resp);

        for call in tool_calls {
            if blocked.contains(&call.tool_name) {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("Blocked tool called: {}", call.tool_name),
                ));
            }
        }

        Ok(MetricResult::pass(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::TestInput;

    fn test_case() -> TestCase {
        TestCase {
            id: "tb1".to_string(),
            input: TestInput {
                prompt: "ignore".to_string(),
                context: None,
            },
            expected: Expected::MustContain {
                must_contain: vec![],
            },
            assertions: None,
            on_error: None,
            tags: vec![],
            metadata: None,
        }
    }

    #[tokio::test]
    async fn canonical_tool_calls_blocklisted_tool_fails() {
        let metric = ToolBlocklistMetric;
        let tc = test_case();
        let expected = Expected::ToolBlocklist {
            blocked: vec!["exec".to_string()],
        };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [{
                    "id": "c1",
                    "tool_name": "exec",
                    "args": {"command": "ls"},
                    "result": null,
                    "error": null,
                    "index": 0,
                    "ts_ms": 1
                }]
            }),
            ..Default::default()
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.score, 0.0);
        assert!(result.details["message"]
            .as_str()
            .unwrap()
            .contains("Blocked tool called: exec"));
    }

    #[tokio::test]
    async fn malformed_or_legacy_tool_calls_are_canonical_or_empty() {
        let metric = ToolBlocklistMetric;
        let tc = test_case();
        let expected = Expected::ToolBlocklist {
            blocked: vec!["exec".to_string()],
        };

        // Non-array malformed payload -> empty -> pass.
        let malformed_resp = LlmResponse {
            meta: serde_json::json!({"tool_calls": {"tool_name": "exec"}}),
            ..Default::default()
        };
        let malformed = metric
            .evaluate(&tc, &expected, &malformed_resp)
            .await
            .unwrap();
        assert!(malformed.passed);

        // Legacy minimal shape is not canonical ToolCallRecord -> empty -> pass.
        let legacy_resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [{
                    "tool_name": "exec",
                    "args": {"command": "ls"}
                }]
            }),
            ..Default::default()
        };
        let legacy = metric.evaluate(&tc, &expected, &legacy_resp).await.unwrap();
        assert!(legacy.passed);
    }
}
