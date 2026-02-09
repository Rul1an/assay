use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;

pub struct ArgsValidMetric;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnconstrainedMode {
    Warn,
    Deny,
    Allow,
}

#[derive(Debug, Clone)]
struct StructuredPolicy {
    allow: Vec<String>,
    deny: Vec<String>,
    schemas: HashMap<String, serde_json::Value>,
    unconstrained: UnconstrainedMode,
}

#[derive(Debug, Clone)]
enum PolicySource {
    SchemaMap(HashMap<String, serde_json::Value>),
    Structured(StructuredPolicy),
}

fn parse_tool_call_entry(v: &serde_json::Value, idx: usize) -> Option<ToolCallRecord> {
    if let Ok(call) = serde_json::from_value::<ToolCallRecord>(v.clone()) {
        return Some(call);
    }
    let obj = v.as_object()?;
    let tool_name = obj
        .get("tool_name")
        .or(obj.get("tool"))
        .and_then(|x| x.as_str())
        .map(ToString::to_string)?;

    let args = obj
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let id = obj
        .get("id")
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("legacy-{}", idx));
    let index = obj
        .get("index")
        .and_then(|x| x.as_u64())
        .map(|x| x as usize)
        .unwrap_or(idx);
    let ts_ms = obj
        .get("ts_ms")
        .or(obj.get("timestamp"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let result = obj.get("result").cloned();
    let error = obj.get("error").cloned();

    Some(ToolCallRecord {
        id,
        tool_name,
        args,
        result,
        error,
        index,
        ts_ms,
    })
}

fn extract_tool_calls(resp: &LlmResponse) -> Vec<ToolCallRecord> {
    let Some(val) = resp.meta.get("tool_calls") else {
        return Vec::new();
    };
    if let Ok(calls) = serde_json::from_value::<Vec<ToolCallRecord>>(val.clone()) {
        return calls;
    }
    val.as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, entry)| parse_tool_call_entry(entry, idx))
                .collect()
        })
        .unwrap_or_default()
}

fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return tool_name == pattern;
    }
    let starts_star = pattern.starts_with('*');
    let ends_star = pattern.ends_with('*');
    match (starts_star, ends_star) {
        (true, true) => {
            let inner = pattern.trim_matches('*');
            if inner.is_empty() {
                true
            } else {
                tool_name.contains(inner)
            }
        }
        (false, true) => {
            let prefix = pattern.trim_end_matches('*');
            !prefix.is_empty() && tool_name.starts_with(prefix)
        }
        (true, false) => {
            let suffix = pattern.trim_start_matches('*');
            !suffix.is_empty() && tool_name.ends_with(suffix)
        }
        (false, false) => tool_name == pattern,
    }
}

fn extract_string_list(val: Option<&serde_json::Value>) -> Vec<String> {
    val.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_unconstrained_mode(policy_json: &serde_json::Value) -> UnconstrainedMode {
    match policy_json
        .pointer("/enforcement/unconstrained_tools")
        .and_then(|v| v.as_str())
    {
        Some("deny") => UnconstrainedMode::Deny,
        Some("allow") => UnconstrainedMode::Allow,
        _ => UnconstrainedMode::Warn,
    }
}

fn has_structured_policy_shape(root: &serde_json::Value) -> bool {
    [
        "version",
        "name",
        "tools",
        "allow",
        "deny",
        "schemas",
        "constraints",
        "enforcement",
        "limits",
        "signatures",
        "tool_pins",
        "discovery",
        "runtime_monitor",
        "kill_switch",
    ]
    .iter()
    .any(|k| root.get(k).is_some())
}

fn load_policy_source(path: &Path) -> anyhow::Result<PolicySource> {
    let policy_content = std::fs::read_to_string(path).map_err(|e| {
        anyhow::anyhow!(
            "config error: failed to read args_valid policy '{}': {}",
            path.display(),
            e
        )
    })?;

    let policy_json: serde_json::Value = serde_yaml::from_str(&policy_content)
        .map_err(|e| anyhow::anyhow!("config error: invalid args_valid policy YAML: {}", e))?;

    if has_structured_policy_shape(&policy_json) {
        let allow = {
            let mut merged = extract_string_list(policy_json.get("allow"));
            merged.extend(extract_string_list(policy_json.pointer("/tools/allow")));
            merged
        };
        let deny = {
            let mut merged = extract_string_list(policy_json.get("deny"));
            merged.extend(extract_string_list(policy_json.pointer("/tools/deny")));
            merged
        };
        let schemas = policy_json
            .get("schemas")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<String, serde_json::Value>>()
            })
            .unwrap_or_default();

        Ok(PolicySource::Structured(StructuredPolicy {
            allow,
            deny,
            schemas,
            unconstrained: parse_unconstrained_mode(&policy_json),
        }))
    } else {
        let schemas: HashMap<String, serde_json::Value> = serde_yaml::from_str(&policy_content)
            .map_err(|e| anyhow::anyhow!("config error: invalid args_valid policy YAML: {}", e))?;
        Ok(PolicySource::SchemaMap(schemas))
    }
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
            static WARN_ONCE: std::sync::Once = std::sync::Once::new();
            WARN_ONCE.call_once(|| {
                if std::env::var("MCP_CONFIG_LEGACY").is_err() {
                    eprintln!(
                        "WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.",
                        path
                    );
                    eprintln!(
                        "      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'."
                    );
                }
            });

            load_policy_source(Path::new(path))?
        } else {
            return Ok(MetricResult::pass(1.0));
        };

        // No calls -> valid args (vacuously true)
        let tool_calls: Vec<ToolCallRecord> = extract_tool_calls(resp);

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

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::{TestInput, ToolCallRecord};
    use std::fs::OpenOptions;
    use std::io::{ErrorKind, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_expected_with_policy(path: &str) -> Expected {
        Expected::ArgsValid {
            policy: Some(path.to_string()),
            schema: None,
        }
    }

    fn make_test_case() -> TestCase {
        TestCase {
            id: "t1".to_string(),
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

    fn make_response_with_tool(tool_name: &str, args: serde_json::Value) -> LlmResponse {
        let call = ToolCallRecord {
            id: "1".to_string(),
            tool_name: tool_name.to_string(),
            args,
            result: None,
            error: None,
            index: 0,
            ts_ms: 0,
        };
        let mut meta = serde_json::Map::new();
        meta.insert(
            "tool_calls".to_string(),
            serde_json::to_value(vec![call]).unwrap(),
        );
        LlmResponse {
            meta: serde_json::Value::Object(meta),
            ..Default::default()
        }
    }

    fn write_temp_policy(contents: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let pid = std::process::id();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for attempt in 0..32u32 {
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "assay_args_valid_{}_{}_{}_{}.yaml",
                pid, ts, seq, attempt
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(contents.as_bytes()).unwrap();
                    return path.to_string_lossy().to_string();
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp policy file: {err}"),
            }
        }
        panic!("failed to allocate unique temp policy file name after retries");
    }

    #[tokio::test]
    async fn structured_policy_denies_tool_from_denylist() {
        let policy_path = write_temp_policy(
            r#"version: "2.0"
tools:
  allow: ["*"]
  deny: ["exec"]
schemas:
  read_file:
    type: object
    required: ["path"]
    properties:
      path:
        type: string
"#,
        );

        let metric = ArgsValidMetric;
        let tc = make_test_case();
        let expected = make_expected_with_policy(&policy_path);
        let resp = make_response_with_tool("exec", serde_json::json!({"command":"rm -rf /"}));

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.score, 0.0);
        assert!(
            result.details.to_string().contains("E_TOOL_DENIED"),
            "details={}",
            result.details
        );

        let _ = std::fs::remove_file(policy_path);
    }

    #[tokio::test]
    async fn legacy_schema_map_keeps_missing_tool_compat() {
        let policy_path = write_temp_policy(
            r#"read_file:
  type: object
  required: ["path"]
  properties:
    path:
      type: string
"#,
        );

        let metric = ArgsValidMetric;
        let tc = make_test_case();
        let expected = make_expected_with_policy(&policy_path);
        let resp = make_response_with_tool("exec", serde_json::json!({"command":"ls"}));

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(
            result.passed,
            "legacy schema-only policy should remain compat"
        );

        let _ = std::fs::remove_file(policy_path);
    }

    #[tokio::test]
    async fn structured_policy_applies_to_minimal_legacy_tool_call_shape() {
        let policy_path = write_temp_policy(
            r#"version: "2.0"
tools:
  allow: ["*"]
  deny: ["exec"]
schemas:
  read_file:
    type: object
    required: ["path"]
    properties:
      path:
        type: string
"#,
        );

        let metric = ArgsValidMetric;
        let tc = make_test_case();
        let expected = make_expected_with_policy(&policy_path);
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [
                    {
                        "tool_name": "exec",
                        "args": {"command": "rm -rf /"}
                    }
                ]
            }),
            ..Default::default()
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(
            !result.passed,
            "minimal tool_call shape must still be enforced"
        );
        assert!(
            result.details.to_string().contains("E_TOOL_DENIED"),
            "details={}",
            result.details
        );

        let _ = std::fs::remove_file(policy_path);
    }

    #[test]
    fn extract_tool_calls_best_effort_preserves_order_and_field_mapping() {
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [
                    {
                        "id": "c0",
                        "tool_name": "alpha",
                        "args": {"k":"v"},
                        "result": {"ok": true},
                        "error": "none",
                        "index": 7,
                        "ts_ms": 42
                    },
                    {
                        "tool": "beta",
                        "args": ["x"],
                        "error": {"code": "E_FAIL"}
                    },
                    {
                        "args": {"missing_tool": true}
                    }
                ]
            }),
            ..Default::default()
        };

        let calls = extract_tool_calls(&resp);
        assert_eq!(calls.len(), 2, "non-parseable entries must be skipped");

        assert_eq!(calls[0].tool_name, "alpha");
        assert_eq!(calls[0].id, "c0");
        assert_eq!(calls[0].index, 7);
        assert_eq!(calls[0].ts_ms, 42);
        assert_eq!(calls[0].args, serde_json::json!({"k":"v"}));
        assert_eq!(calls[0].result, Some(serde_json::json!({"ok": true})));
        assert_eq!(calls[0].error, Some(serde_json::json!("none")));

        assert_eq!(calls[1].tool_name, "beta");
        assert_eq!(calls[1].id, "legacy-1");
        assert_eq!(calls[1].index, 1);
        assert_eq!(calls[1].ts_ms, 0);
        assert_eq!(calls[1].args, serde_json::json!(["x"]));
        assert_eq!(calls[1].result, None);
        assert_eq!(calls[1].error, Some(serde_json::json!({"code":"E_FAIL"})));
    }

    #[test]
    fn extract_tool_calls_best_effort_returns_empty_for_missing_or_non_array_tool_calls() {
        let missing = LlmResponse {
            meta: serde_json::json!({}),
            ..Default::default()
        };
        assert!(extract_tool_calls(&missing).is_empty());

        let non_array = LlmResponse {
            meta: serde_json::json!({"tool_calls": {"tool_name": "x"}}),
            ..Default::default()
        };
        assert!(extract_tool_calls(&non_array).is_empty());
    }
}
