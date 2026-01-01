//! MCP tool: check_coverage
//!
//! Analyzes trace coverage against a policy and returns coverage metrics.

use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

/// Input for coverage analysis
#[derive(Debug, serde::Deserialize)]
struct CoverageInput {
    /// Policy file path (relative to policy root)
    policy: String,

    /// Traces to analyze
    traces: Vec<TraceInput>,

    /// Minimum coverage threshold (0-100), default 80
    #[serde(default = "default_threshold")]
    threshold: f64,

    /// Output format: "json", "markdown", "github"
    #[serde(default = "default_format")]
    format: String,
}

#[derive(Debug, serde::Deserialize)]
struct TraceInput {
    /// Trace identifier
    #[serde(default)]
    id: String,

    /// Tools called in this trace
    tools: Vec<String>,

    /// Rules that were triggered (optional, for rule coverage)
    #[serde(default)]
    rules_triggered: Vec<String>,
}

fn default_threshold() -> f64 {
    80.0
}

fn default_format() -> String {
    "json".to_string()
}

pub async fn check_coverage(ctx: &ToolContext, args: &Value) -> Result<Value> {
    // Parse input
    let input: CoverageInput =
        serde_json::from_value(args.clone()).context("Invalid coverage input")?;

    // Validate threshold
    if input.threshold < 0.0 || input.threshold > 100.0 {
        return ToolError::new("E_INVALID_THRESHOLD", "Threshold must be between 0 and 100")
            .result();
    }

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

    // Parse policy
    let policy: assay_core::model::Policy = match serde_yaml::from_slice(&policy_bytes) {
        Ok(p) => p,
        Err(e) => {
            return ToolError::new("E_POLICY_PARSE", &format!("Failed to parse policy: {}", e))
                .result();
        }
    };

    // Convert input traces to internal format
    let traces: Vec<assay_core::coverage::TraceRecord> = input
        .traces
        .into_iter()
        .enumerate()
        .map(|(idx, t)| assay_core::coverage::TraceRecord {
            trace_id: if t.id.is_empty() {
                format!("trace_{}", idx)
            } else {
                t.id
            },
            tools_called: t.tools,
            rules_triggered: t.rules_triggered.into_iter().collect(),
        })
        .collect();

    // Analyze coverage
    let analyzer = assay_core::coverage::CoverageAnalyzer::from_policy(&policy);
    let report = analyzer.analyze(&traces, input.threshold);

    // Format output
    match input.format.as_str() {
        "markdown" => Ok(serde_json::json!({
            "format": "markdown",
            "content": report.to_markdown(),
            "meets_threshold": report.meets_threshold,
            "overall_coverage_pct": report.overall_coverage_pct,
        })),
        "github" => Ok(serde_json::json!({
            "format": "github",
            "annotations": report.to_github_annotation(),
            "meets_threshold": report.meets_threshold,
            "overall_coverage_pct": report.overall_coverage_pct,
        })),
        _ => {
            // Default: full JSON report
            Ok(serde_json::to_value(&report)?)
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

    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    async fn setup_test(policy_yaml: &str) -> (ToolContext, std::path::PathBuf) {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!("assay_cov_test_{}_{}", unique_id, count));
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
    async fn test_full_coverage_analysis() {
        let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - SearchKnowledgeBase
    - CreateTicket
sequences:
  - type: before
    first: SearchKnowledgeBase
    then: CreateTicket
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "traces": [
                {
                    "id": "trace1",
                    "tools": ["SearchKnowledgeBase", "CreateTicket"],
                    "rules_triggered": ["before_searchknowledgebase_then_createticket"]
                }
            ],
            "threshold": 80.0
        });

        let result = check_coverage(&ctx, &args).await.unwrap();
        // println!("DEBUG: Coverage Result: {}", serde_json::to_string_pretty(&result).unwrap());

        assert!(result["meets_threshold"].as_bool().unwrap());
        assert!(result["tool_coverage"]["coverage_pct"].as_f64().unwrap() > 80.0);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_low_coverage_fails_threshold() {
        let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - Tool1
    - Tool2
    - Tool3
    - Tool4
    - Tool5
sequences: []
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "traces": [
                {
                    "tools": ["Tool1"]  // Only 1 of 5 tools
                }
            ],
            "threshold": 80.0
        });

        let result = check_coverage(&ctx, &args).await.unwrap();

        assert!(!result["meets_threshold"].as_bool().unwrap());
        assert_eq!(
            result["tool_coverage"]["tools_seen_in_traces"]
                .as_u64()
                .unwrap(),
            1
        );

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_markdown_output() {
        let policy = r#"
version: "1.1"
name: "test"
tools:
  allow: [Search]
sequences: []
"#;

        let (ctx, temp_dir) = setup_test(policy).await;

        let args = json!({
            "policy": "policy.yaml",
            "traces": [{ "tools": ["Search"] }],
            "format": "markdown"
        });

        let result = check_coverage(&ctx, &args).await.unwrap();

        assert_eq!(result["format"], "markdown");
        assert!(result["content"]
            .as_str()
            .unwrap()
            .contains("## Coverage Report"));

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }
}
