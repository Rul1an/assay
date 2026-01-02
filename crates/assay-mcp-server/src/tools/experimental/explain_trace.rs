//! MCP tool: explain_trace
//!
//! Provides step-by-step explanation of trace evaluation against a policy.

use crate::tools::{ToolContext, ToolError};
use anyhow::{Context, Result};
use assay_core::experimental::explain;
use serde_json::Value;

#[derive(Debug, serde::Deserialize)]
struct ExplainInput {
    /// Policy file path
    policy: String,

    /// Trace to explain (array of tool calls)
    trace: Vec<ToolCallInput>,

    /// Output format: json, markdown, html, terminal
    #[serde(default = "default_format")]
    format: String,

    /// Include verbose rule evaluation details
    #[serde(default)]
    _verbose: bool,
}

#[derive(Debug, serde::Deserialize)]
struct ToolCallInput {
    #[serde(alias = "name", alias = "tool_name")]
    tool: String,

    #[serde(default, alias = "arguments", alias = "parameters")]
    args: Option<serde_json::Value>,
}

fn default_format() -> String {
    "json".to_string()
}

pub async fn explain_trace(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let input: ExplainInput =
        serde_json::from_value(args.clone()).context("Invalid explain input")?;

    // Load policy
    let policy_path = match ctx.resolve_policy_path(&input.policy).await {
        Ok(p) => p,
        Err(e) => return e.result(),
    };

    let policy_bytes = match tokio::fs::read(&policy_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ToolError::new(
                "E_POLICY_NOT_FOUND",
                &format!("Policy not found: {}", input.policy),
            )
            .result();
        }
        Err(e) => return ToolError::new("E_POLICY_READ", &e.to_string()).result(),
    };

    let policy: assay_core::model::Policy = match serde_yaml::from_slice(&policy_bytes) {
        Ok(p) => p,
        Err(e) => {
            return ToolError::new("E_POLICY_PARSE", &format!("Failed to parse policy: {}", e))
                .result();
        }
    };

    // Convert input to ToolCall
    let trace: Vec<explain::ToolCall> = input
        .trace
        .into_iter()
        .map(|t| explain::ToolCall {
            tool: t.tool,
            args: t.args,
        })
        .collect();

    // Run explanation
    let explainer = explain::TraceExplainer::new(policy);
    let explanation = explainer.explain(&trace);

    // Format output
    match input.format.as_str() {
        "markdown" | "md" => Ok(serde_json::json!({
            "format": "markdown",
            "content": explanation.to_markdown(),
            "blocked_steps": explanation.blocked_steps,
            "total_steps": explanation.total_steps
        })),
        "html" => Ok(serde_json::json!({
            "format": "html",
            "content": explanation.to_html(),
            "blocked_steps": explanation.blocked_steps,
            "total_steps": explanation.total_steps
        })),
        "terminal" | "text" => Ok(serde_json::json!({
            "format": "terminal",
            "content": explanation.to_terminal(),
            "blocked_steps": explanation.blocked_steps,
            "total_steps": explanation.total_steps
        })),
        _ => {
            // Default: full JSON
            Ok(serde_json::to_value(&explanation)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PolicyCaches;
    use crate::config::ServerConfig;
    use crate::tools::ToolContext;
    use serde_json::json;

    async fn setup_test(policy_yaml: &str) -> (ToolContext, std::path::PathBuf) {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("assay_explain_test_{}", unique_id));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let policy_path = temp_dir.join("policy.yaml");
        tokio::fs::write(&policy_path, policy_yaml).await.unwrap();

        let cfg = ServerConfig::default();
        let caches = PolicyCaches::new(100);
        let policy_root_canon = tokio::fs::canonicalize(&temp_dir).await.unwrap();

        let ctx = ToolContext {
            policy_root: temp_dir.clone(),
            policy_root_canon,
            cfg,
            caches,
        };

        (ctx, temp_dir)
    }

    #[tokio::test]
    async fn test_explain_allowed_trace() {
        let policy = r#"
version: "1.1"
name: "test"
sequences:
  - type: before
    first: Search
    then: Create
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "trace": [
                {"tool": "Search"},
                {"tool": "Create"}
            ]
        });

        let result = explain_trace(&ctx, &args).await.unwrap();

        assert_eq!(result["blocked_steps"], 0);
        assert_eq!(result["allowed_steps"], 2);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_explain_blocked_trace() {
        let policy = r#"
version: "1.1"
name: "test"
sequences:
  - type: before
    first: Search
    then: Create
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "trace": [
                {"tool": "Create"}  // Blocked - no Search first
            ]
        });

        let result = explain_trace(&ctx, &args).await.unwrap();

        assert_eq!(result["blocked_steps"], 1);
        assert!(result["first_block_index"].as_u64().is_some());

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_explain_markdown_format() {
        let policy = r#"
version: "1.1"
name: "test"
sequences: []
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "trace": [{"tool": "Search"}],
            "format": "markdown"
        });

        let result = explain_trace(&ctx, &args).await.unwrap();

        assert_eq!(result["format"], "markdown");
        assert!(result["content"]
            .as_str()
            .unwrap()
            .contains("## Trace Explanation"));

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }
}
