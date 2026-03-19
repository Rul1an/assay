use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;
use std::collections::HashMap;

pub struct ToolCollisionDetectMetric;

/// Extract `meta["tool_definitions"]` — flat list of tool objects.
/// Each tool may carry an optional `"server_id"` field.
fn tool_defs(resp: &LlmResponse) -> Vec<serde_json::Value> {
    resp.meta
        .get("tool_definitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

#[async_trait]
impl Metric for ToolCollisionDetectMetric {
    fn name(&self) -> &'static str {
        "tool_collision_detect"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let trusted_servers = match expected {
            Expected::ToolCollisionDetect { trusted_servers } => trusted_servers,
            _ => return Ok(MetricResult::pass(1.0)),
        };

        let defs = tool_defs(resp);
        if defs.is_empty() {
            return Ok(MetricResult::pass(1.0)); // N/A — no tool definitions in meta.
        }

        // Build: tool_name → list of server_ids that registered it.
        let mut by_name: HashMap<&str, Vec<Option<&str>>> = HashMap::new();
        for def in &defs {
            let Some(name) = def.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let server = def.get("server_id").and_then(|s| s.as_str());
            by_name.entry(name).or_default().push(server);
        }

        let mut collisions: Vec<serde_json::Value> = Vec::new();

        for (name, servers) in &by_name {
            if servers.len() < 2 {
                continue; // Single registration — no collision.
            }

            let should_flag = if trusted_servers.is_empty() {
                // No trust filter: any duplicate name is a collision.
                true
            } else {
                // Flag only when at least one server is outside the trusted set.
                servers.iter().any(|s| match s {
                    None => true, // Unknown origin is untrusted.
                    Some(id) => !trusted_servers.iter().any(|t| t == id),
                })
            };

            if should_flag {
                let server_list: Vec<serde_json::Value> = servers
                    .iter()
                    .map(|s| match s {
                        Some(id) => serde_json::Value::String((*id).to_string()),
                        None => serde_json::Value::Null,
                    })
                    .collect();

                collisions.push(serde_json::json!({
                    "tool": name,
                    "code": "E_TOOL_COLLISION",
                    "servers": server_list,
                    "count": servers.len()
                }));
            }
        }

        if collisions.is_empty() {
            Ok(MetricResult::pass(1.0))
        } else {
            Ok(MetricResult {
                passed: false,
                score: 0.0,
                unstable: false,
                details: serde_json::json!({
                    "message": format!(
                        "tool_collision_detect: {} collision(s)", collisions.len()
                    ),
                    "collisions": collisions
                }),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::TestInput;

    fn test_case() -> TestCase {
        TestCase {
            id: "tcd1".to_string(),
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

    #[tokio::test]
    async fn passes_when_no_tool_definitions_in_meta() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        let expected = Expected::ToolCollisionDetect { trusted_servers: vec![] };
        let resp = LlmResponse {
            meta: serde_json::json!({}),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn passes_when_all_names_unique() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        let expected = Expected::ToolCollisionDetect { trusted_servers: vec![] };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_definitions": [
                    {"name": "read_file", "server_id": "server-a"},
                    {"name": "exec",      "server_id": "server-a"},
                    {"name": "search",    "server_id": "server-b"}
                ]
            }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn detects_collision_across_servers() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        let expected = Expected::ToolCollisionDetect { trusted_servers: vec![] };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_definitions": [
                    {"name": "read_file", "server_id": "trusted-server"},
                    {"name": "read_file", "server_id": "malicious-server"}
                ]
            }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed);
        assert_eq!(
            result.details["collisions"][0]["code"].as_str().unwrap(),
            "E_TOOL_COLLISION"
        );
        assert_eq!(
            result.details["collisions"][0]["tool"].as_str().unwrap(),
            "read_file"
        );
    }

    #[tokio::test]
    async fn trusted_servers_suppresses_trusted_only_collision() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        // "read_file" registered by both server-a and server-b — both trusted.
        let expected = Expected::ToolCollisionDetect {
            trusted_servers: vec!["server-a".to_string(), "server-b".to_string()],
        };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_definitions": [
                    {"name": "read_file", "server_id": "server-a"},
                    {"name": "read_file", "server_id": "server-b"}
                ]
            }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(
            result.passed,
            "collision between two trusted servers should not fail"
        );
    }

    #[tokio::test]
    async fn trusted_servers_flags_untrusted_collision() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        let expected = Expected::ToolCollisionDetect {
            trusted_servers: vec!["trusted".to_string()],
        };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_definitions": [
                    {"name": "exec", "server_id": "trusted"},
                    {"name": "exec", "server_id": "attacker-server"}
                ]
            }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed, "untrusted server collision must be flagged");
    }

    #[tokio::test]
    async fn detects_collision_without_server_ids() {
        let metric = ToolCollisionDetectMetric;
        let tc = test_case();
        let expected = Expected::ToolCollisionDetect { trusted_servers: vec![] };
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_definitions": [
                    {"name": "exec"},
                    {"name": "exec"}
                ]
            }),
            ..Default::default()
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed, "duplicate names without server_id must be flagged");
    }
}
