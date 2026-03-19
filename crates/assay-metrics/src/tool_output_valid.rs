use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

use crate::tool_calls::extract_tool_calls_best_effort;

pub struct ToolOutputValidMetric;

#[async_trait]
impl Metric for ToolOutputValidMetric {
    fn name(&self) -> &'static str {
        "tool_output_valid"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let schemas = match expected {
            Expected::ToolOutputValid { schemas } => schemas,
            _ => return Ok(MetricResult::pass(1.0)),
        };

        let Some(schemas) = schemas else {
            return Ok(MetricResult::pass(1.0)); // N/A — no schemas configured.
        };

        let tool_calls = extract_tool_calls_best_effort(resp);
        let mut violations: Vec<serde_json::Value> = Vec::new();

        for call in &tool_calls {
            let Some(schema) = schemas.get(&call.tool_name) else {
                continue; // No schema for this tool — skip.
            };

            let Some(result) = &call.result else {
                continue; // No output to validate.
            };

            // Error outputs carry no semantic contract — skip validation.
            if call.error.is_some() {
                continue;
            }

            let compiled = jsonschema::options()
                .build(schema)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "config error: invalid output schema for tool '{}': {}",
                        call.tool_name,
                        e
                    )
                })?;

            if !compiled.is_valid(result) {
                let errors: Vec<String> =
                    compiled.iter_errors(result).map(|e| e.to_string()).collect();
                violations.push(serde_json::json!({
                    "tool": call.tool_name,
                    "call_id": call.id,
                    "code": "E_OUTPUT_SCHEMA_VIOLATION",
                    "errors": errors
                }));
            }
        }

        if violations.is_empty() {
            Ok(MetricResult::pass(1.0))
        } else {
            Ok(MetricResult {
                passed: false,
                score: 0.0,
                unstable: false,
                details: serde_json::json!({
                    "message": format!(
                        "tool_output_valid: {} violation(s)", violations.len()
                    ),
                    "violations": violations
                }),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::{TestInput, ToolCallRecord};

    fn test_case() -> TestCase {
        TestCase {
            id: "tov1".to_string(),
            input: TestInput {
                prompt: "ignore".to_string(),
                context: None,
            },
            expected: Expected::default(),
            assertions: None,
            on_error: None,
            tags: vec![],
            metadata: None,
        }
    }

    fn resp_with_result(tool_name: &str, result: serde_json::Value) -> LlmResponse {
        let call = ToolCallRecord {
            id: "c1".to_string(),
            tool_name: tool_name.to_string(),
            args: serde_json::json!({}),
            result: Some(result),
            error: None,
            index: 0,
            ts_ms: 0,
        };
        LlmResponse {
            meta: serde_json::json!({ "tool_calls": [call] }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn passes_when_no_schemas_configured() {
        let metric = ToolOutputValidMetric;
        let tc = test_case();
        let expected = Expected::ToolOutputValid { schemas: None };
        let resp = resp_with_result("exec", serde_json::json!({"exit_code": 0}));
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn passes_when_output_matches_schema() {
        let metric = ToolOutputValidMetric;
        let tc = test_case();
        let expected = Expected::ToolOutputValid {
            schemas: Some(serde_json::json!({
                "exec": {
                    "type": "object",
                    "required": ["exit_code"],
                    "properties": {
                        "exit_code": {"type": "integer"},
                        "stdout": {"type": "string"}
                    }
                }
            })),
        };
        let resp = resp_with_result("exec", serde_json::json!({"exit_code": 0, "stdout": "ok"}));
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn fails_when_output_violates_schema() {
        let metric = ToolOutputValidMetric;
        let tc = test_case();
        let expected = Expected::ToolOutputValid {
            schemas: Some(serde_json::json!({
                "exec": {
                    "type": "object",
                    "required": ["exit_code"],
                    "properties": {
                        "exit_code": {"type": "integer"}
                    }
                }
            })),
        };
        // Missing required `exit_code`.
        let resp = resp_with_result("exec", serde_json::json!({"stdout": "ok"}));
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed);
        assert_eq!(
            result.details["violations"][0]["code"].as_str().unwrap(),
            "E_OUTPUT_SCHEMA_VIOLATION"
        );
    }

    #[tokio::test]
    async fn skips_tool_without_schema() {
        let metric = ToolOutputValidMetric;
        let tc = test_case();
        let expected = Expected::ToolOutputValid {
            schemas: Some(serde_json::json!({
                "read_file": {"type": "object"}
            })),
        };
        // Tool "exec" has no schema — should not be checked.
        let resp = resp_with_result("exec", serde_json::json!("anything goes"));
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn skips_error_results() {
        let metric = ToolOutputValidMetric;
        let tc = test_case();
        let expected = Expected::ToolOutputValid {
            schemas: Some(serde_json::json!({
                "exec": {
                    "type": "object",
                    "required": ["exit_code"],
                    "properties": {"exit_code": {"type": "integer"}}
                }
            })),
        };
        // Error result with missing field — should be skipped.
        let call = ToolCallRecord {
            id: "c1".to_string(),
            tool_name: "exec".to_string(),
            args: serde_json::json!({}),
            result: Some(serde_json::json!({})),
            error: Some(serde_json::json!({"message": "timeout"})),
            index: 0,
            ts_ms: 0,
        };
        let resp = LlmResponse {
            meta: serde_json::json!({ "tool_calls": [call] }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed, "error outputs must not be schema-validated");
    }
}
