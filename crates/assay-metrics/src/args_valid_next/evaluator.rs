use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{validated_args_policy, ArgsPolicyView};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;

use super::matcher::matches_tool_pattern;
use super::policy::policy_input;
use crate::tool_calls::extract_tool_calls_best_effort;

pub struct ArgsValidMetric;

/// Schema violations as the policy engine records them, each naming the tool whose call it is.
/// The engine records the argument path and the failed constraint, not the tool, and a record
/// that two tools' calls can produce does not say which call failed.
fn naming_tool<'a>(
    violations: &'a [serde_json::Value],
    tool: &'a str,
) -> impl Iterator<Item = serde_json::Value> + 'a {
    violations.iter().map(move |violation| {
        let mut violation = violation.clone();
        if let Some(record) = violation.as_object_mut() {
            record
                .entry("tool")
                .or_insert_with(|| serde_json::Value::String(tool.to_string()));
        }
        violation
    })
}

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
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "a metric declines every Expected variant but its own, and now says so with not_applicable() rather than a vacuous pass (#1949 layer 2)"
        )]
        let (policy_path, inline_schema) = match expected {
            Expected::ArgsValid { policy, schema } => (policy, schema),
            _ => return Ok(MetricResult::not_applicable()),
        };
        let input = policy_input(inline_schema.as_ref(), policy_path.as_deref(), self.name())?;
        let policy_source = validated_args_policy(&input)?;

        // No calls -> valid args (vacuously true)
        let tool_calls: Vec<ToolCallRecord> = match extract_tool_calls_best_effort(resp) {
            Ok(tool_calls) => tool_calls,
            Err(_) => {
                return Ok(MetricResult::fail(
                    0.0,
                    "args_valid could not read tool-call evidence",
                ));
            }
        };

        let mut errors: Vec<serde_json::Value> = Vec::new();

        match policy_source {
            ArgsPolicyView::SchemaMap(schemas) => {
                let state = assay_core::policy_engine::PolicyState::compile(schemas);

                for call in tool_calls {
                    let verdict = state.evaluate(&call.tool_name, &call.args);

                    if verdict.status == assay_core::policy_engine::VerdictStatus::Blocked {
                        // Legacy compat: schema-only mode ignores missing tools.
                        if verdict.reason_code == "E_ARG_SCHEMA" {
                            if let Some(violations) =
                                verdict.details.get("violations").and_then(|v| v.as_array())
                            {
                                errors.extend(naming_tool(violations, &call.tool_name));
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
            ArgsPolicyView::Structured {
                allow,
                deny,
                schemas,
                deny_unconstrained,
            } => {
                let empty = serde_json::json!({});
                let schemas = schemas.as_ref().unwrap_or(&empty);
                let state = assay_core::policy_engine::PolicyState::compile(schemas);

                for call in tool_calls {
                    if deny
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

                    if !allow.is_empty()
                        && !allow
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

                    if schemas.get(&call.tool_name).is_none() {
                        if deny_unconstrained {
                            errors.push(serde_json::json!({
                                "code": "E_TOOL_UNCONSTRAINED",
                                "tool": call.tool_name,
                                "message": "Tool has no schema (enforcement: deny)"
                            }));
                        }
                        continue;
                    }

                    let verdict = state.evaluate(&call.tool_name, &call.args);
                    if verdict.status == assay_core::policy_engine::VerdictStatus::Blocked {
                        if verdict.reason_code == "E_ARG_SCHEMA" {
                            if let Some(violations) =
                                verdict.details.get("violations").and_then(|v| v.as_array())
                            {
                                errors.extend(naming_tool(violations, &call.tool_name));
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
                exercised: assay_core::metrics_api::Exercised::Exercised,
                passed: false,
                score: 0.0,
                details: serde_json::Value::Object(details),
                unstable: false,
            })
        }
    }
}
