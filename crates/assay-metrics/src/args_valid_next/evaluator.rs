use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;

use super::matcher::matches_tool_pattern;
use super::policy::{load_policy_source, PolicySource, UnconstrainedMode};
use crate::policy_warning::should_emit_deprecated_policy_warning;
use crate::tool_calls::extract_tool_calls_best_effort;

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

        let policy_source = if let Some(schema) = inline_schema {
            let schemas: HashMap<String, serde_json::Value> =
                serde_json::from_value(schema.clone()).map_err(|e| {
                    anyhow::anyhow!("config error: invalid inline args_valid schema: {}", e)
                })?;
            PolicySource::SchemaMap(schemas)
        } else if let Some(path) = policy_path {
            if should_emit_deprecated_policy_warning(self.name(), path) {
                eprintln!(
                    "WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.",
                    path
                );
                eprintln!(
                    "      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'."
                );
            }

            load_policy_source(Path::new(path))?
        } else {
            return Ok(MetricResult::pass(1.0));
        };

        // No calls -> valid args (vacuously true)
        let tool_calls: Vec<ToolCallRecord> = extract_tool_calls_best_effort(resp);

        let mut errors: Vec<serde_json::Value> = Vec::new();

        match &policy_source {
            PolicySource::SchemaMap(schemas) => {
                let policy_val = serde_json::Value::Object(
                    schemas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );

                for call in tool_calls {
                    let verdict = assay_core::policy_engine::evaluate_tool_args(
                        &policy_val,
                        &call.tool_name,
                        &call.args,
                    );

                    if verdict.status == assay_core::policy_engine::VerdictStatus::Blocked {
                        // Legacy compat: schema-only mode ignores missing tools.
                        if verdict.reason_code == "E_ARG_SCHEMA" {
                            if let Some(violations) =
                                verdict.details.get("violations").and_then(|v| v.as_array())
                            {
                                errors.extend(violations.clone());
                            }
                        } else if verdict.reason_code != "E_POLICY_MISSING_TOOL" {
                            errors.push(serde_json::json!({
                                "code": verdict.reason_code,
                                "message": format!(
                                    "Policy error for {}: {} ({})",
                                    call.tool_name, verdict.reason_code, verdict.details
                                )
                            }));
                        }
                    }
                }
            }
            PolicySource::Structured(policy) => {
                let policy_val = serde_json::Value::Object(
                    policy
                        .schemas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );

                for call in tool_calls {
                    if policy
                        .deny
                        .iter()
                        .any(|p| matches_tool_pattern(&call.tool_name, p))
                    {
                        errors.push(serde_json::json!({
                            "code": "E_TOOL_DENIED",
                            "tool": call.tool_name,
                            "message": "Tool is explicitly denylisted"
                        }));
                        continue;
                    }

                    if !policy.allow.is_empty()
                        && !policy
                            .allow
                            .iter()
                            .any(|p| matches_tool_pattern(&call.tool_name, p))
                    {
                        errors.push(serde_json::json!({
                            "code": "E_TOOL_NOT_ALLOWED",
                            "tool": call.tool_name,
                            "message": "Tool is not in allowlist"
                        }));
                        continue;
                    }

                    if !policy.schemas.contains_key(&call.tool_name) {
                        if policy.unconstrained == UnconstrainedMode::Deny {
                            errors.push(serde_json::json!({
                                "code": "E_TOOL_UNCONSTRAINED",
                                "tool": call.tool_name,
                                "message": "Tool has no schema (enforcement: deny)"
                            }));
                        }
                        continue;
                    }

                    let verdict = assay_core::policy_engine::evaluate_tool_args(
                        &policy_val,
                        &call.tool_name,
                        &call.args,
                    );
                    if verdict.status == assay_core::policy_engine::VerdictStatus::Blocked {
                        if verdict.reason_code == "E_ARG_SCHEMA" {
                            if let Some(violations) =
                                verdict.details.get("violations").and_then(|v| v.as_array())
                            {
                                errors.extend(violations.clone());
                            }
                        } else {
                            errors.push(serde_json::json!({
                                "code": verdict.reason_code,
                                "tool": call.tool_name,
                                "message": format!("Policy error: {}", verdict.reason_code)
                            }));
                        }
                    }
                }
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
