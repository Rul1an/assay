use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;
use jsonschema::JSONSchema;
use std::collections::HashMap;

pub struct ArgsValidMetric;

#[async_trait]
impl Metric for ArgsValidMetric {
    fn name(&self) -> &'static str {
        "args_valid"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let (policy_path, inline_schema) = match expected {
            Expected::ArgsValid { policy, schema } => (policy, schema),
            _ => return Ok(MetricResult::pass(1.0)),
        };

        let schemas: HashMap<String, serde_json::Value> = if let Some(schema) = inline_schema {
            serde_json::from_value(schema.clone()).map_err(|e| {
                anyhow::anyhow!("config error: invalid inline args_valid schema: {}", e)
            })?
        } else if let Some(path) = policy_path {
            static WARN_ONCE: std::sync::Once = std::sync::Once::new();
            WARN_ONCE.call_once(|| {
                 if std::env::var("MCP_CONFIG_LEGACY").is_err() {
                     eprintln!("WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.", path);
                     eprintln!("      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'.");
                 }
             });

            let policy_content = std::fs::read_to_string(path).map_err(|e| {
                anyhow::anyhow!(
                    "config error: failed to read args_valid policy '{}': {}",
                    path,
                    e
                )
            })?;
            serde_yaml::from_str(&policy_content).map_err(|e| {
                anyhow::anyhow!("config error: invalid args_valid policy YAML: {}", e)
            })?
        } else {
            return Ok(MetricResult::pass(1.0));
        };

        let tool_calls: Vec<ToolCallRecord> = if let Some(val) = resp.meta.get("tool_calls") {
            serde_json::from_value(val.clone()).unwrap_or_default()
        } else {
            Vec::new() // No calls -> valid args (vacuously true)
        };

        let mut errors: Vec<serde_json::Value> = Vec::new();

        for call in tool_calls {
            if let Some(schema_json) = schemas.get(&call.tool_name) {
                // Compile schema
                let compiled = JSONSchema::options().compile(schema_json).map_err(|e| {
                    anyhow::anyhow!(
                        "config error: schema compile failed for tool '{}': {}",
                        call.tool_name,
                        e
                    )
                })?;

                let validation_result = compiled.validate(&call.args);
                if let Err(iter) = validation_result {
                    for e in iter {
                        errors.push(serde_json::json!({
                            "field": e.instance_path.to_string(),
                            "constraint": format!("{:?}", e.kind),
                            "suggestion": e.to_string()
                        }));
                    }
                }
            } else {
                // If tool not in policy, do we fail?
                // "ArgsValid" usually implies strictness. If policy acts as an allowlist for schemas.
                // But maybe we only validate *known* tools.
                // Let's decide: If policy is present, unlisted tools = unchecked (use ToolBlocklist for that).
                // Or: unlisted tools = assumption of no args?
                // Let's stick to: only validate if schema is present.
            }
        }

        if errors.is_empty() {
            Ok(MetricResult::pass(1.0))
        } else {
            let mut details = serde_json::Map::new();
            details.insert(
                "message".to_string(),
                serde_json::Value::String(format!("args_valid failed: {} errors", errors.len())),
            );
            details.insert("violations".to_string(), serde_json::Value::Array(errors));

            Ok(MetricResult {
                passed: false,
                score: 0.0,
                details: serde_json::Value::Object(details),
                unstable: false,
            })
        }
    }
}
