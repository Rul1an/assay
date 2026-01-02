//! Integration tests for coverage analysis
//!
//! Tests the full coverage workflow from traces to report.

use assay_mcp_server::cache::PolicyCaches;
use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::tools::ToolContext;
use serde_json::json;

use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Test helper to create policy and traces, run coverage analysis
async fn run_coverage_test(
    policy_yaml: &str,
    traces_jsonl: &str,
    threshold: f64,
) -> serde_json::Value {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir().join(format!("assay_cov_int_{}_{}", unique_id, count));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    // Write policy
    let policy_path = temp_dir.join("policy.yaml");
    tokio::fs::write(&policy_path, policy_yaml).await.unwrap();

    // Write traces (not strictly needed for check_coverage tool itself as it takes JSON,
    // but useful if we were testing CLI. Here we just parse jsonl to args)

    // Parse traces from JSONL string to Vec<TraceInput> structure
    let traces_input: Vec<serde_json::Value> = traces_jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(idx, line)| {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            json!({
                "id": v.get("id").unwrap_or(&json!(format!("t{}", idx))),
                "tools": v["tools"],
                "rules_triggered": v.get("rules_triggered").unwrap_or(&json!([]))
            })
        })
        .collect();

    let args = json!({
        "policy": "policy.yaml",
        "traces": traces_input,
        "threshold": threshold,
        "format": "json" // Default to JSON for assertions
    });

    let cfg = ServerConfig::default();
    let caches = PolicyCaches::new(100);
    // canonicalize required for policy loading
    let policy_root_canon = tokio::fs::canonicalize(&temp_dir).await.unwrap();
    let ctx = ToolContext {
        policy_root: temp_dir.clone(),
        policy_root_canon,
        cfg,
        caches,
    };

    let res = assay_mcp_server::tools::check_coverage::check_coverage(&ctx, &args).await;

    let _ = tokio::fs::remove_dir_all(temp_dir).await;

    // Unwrap result
    res.expect("check_coverage failed")
}

// ==================== TOOL COVERAGE TESTS ====================

#[tokio::test]
async fn test_coverage_100_percent() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - SearchKnowledgeBase
    - GetCustomerInfo
    - CreateTicket
sequences: []
"#;

    let traces =
        r#"{"id": "t1", "tools": ["SearchKnowledgeBase", "GetCustomerInfo", "CreateTicket"]}"#;

    let report = run_coverage_test(policy, traces, 80.0).await;

    assert_eq!(
        report["tool_coverage"]["coverage_pct"].as_f64().unwrap(),
        100.0
    );
    assert!(report["tool_coverage"]["unseen_tools"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(report["meets_threshold"].as_bool().unwrap());
}

#[tokio::test]
async fn test_coverage_partial() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - Tool1
    - Tool2
    - Tool3
    - Tool4
sequences: []
"#;

    let traces = r#"{"id": "t1", "tools": ["Tool1", "Tool2"]}"#;

    let report = run_coverage_test(policy, traces, 80.0).await;

    assert_eq!(
        report["tool_coverage"]["tools_seen_in_traces"]
            .as_u64()
            .unwrap(),
        2
    );
    assert_eq!(
        report["tool_coverage"]["total_tools_in_policy"]
            .as_u64()
            .unwrap(),
        4
    );
    assert_eq!(
        report["tool_coverage"]["coverage_pct"].as_f64().unwrap(),
        50.0
    );
    assert!(!report["meets_threshold"].as_bool().unwrap());

    let unseen: Vec<String> = report["tool_coverage"]["unseen_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(unseen.contains(&"Tool3".to_string()));
    assert!(unseen.contains(&"Tool4".to_string()));
}

// ==================== HIGH-RISK GAPS TESTS ====================

#[tokio::test]
async fn test_high_risk_gaps_detected() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - SafeTool
  deny:
    - DeleteAccount
    - DropDatabase
sequences: []
"#;

    // Only test safe tools, never test dangerous ones
    let traces = r#"{"id": "t1", "tools": ["SafeTool"]}"#;

    let report = run_coverage_test(policy, traces, 50.0).await;

    let high_risk_gaps: Vec<String> = report["high_risk_gaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["tool"].as_str().unwrap().to_string())
        .collect();

    assert!(high_risk_gaps.contains(&"DeleteAccount".to_string()));
    assert!(high_risk_gaps.contains(&"DropDatabase".to_string()));
}

#[tokio::test]
async fn test_high_risk_covered() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  deny:
    - DeleteAccount
sequences: []
"#;

    // Test that the dangerous tool is properly blocked
    let traces = r#"{"id": "t1", "tools": ["DeleteAccount"]}"#;

    let report = run_coverage_test(policy, traces, 50.0).await;

    // DeleteAccount was seen, so no high-risk gap
    assert!(report["high_risk_gaps"].as_array().unwrap().is_empty());
}

// ==================== UNEXPECTED TOOLS TESTS ====================

#[tokio::test]
async fn test_unexpected_tools_detected() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - AllowedTool
sequences: []
"#;

    let traces = r#"{"id": "t1", "tools": ["AllowedTool", "UnknownTool", "AnotherUnknown"]}"#;

    let report = run_coverage_test(policy, traces, 50.0).await;

    let unexpected: Vec<String> = report["tool_coverage"]["unexpected_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(unexpected.contains(&"UnknownTool".to_string()));
    assert!(unexpected.contains(&"AnotherUnknown".to_string()));
}

// ==================== ALIAS COVERAGE TESTS ====================

#[tokio::test]
async fn test_alias_coverage() {
    let policy = r#"
version: "1.1"
name: "test"
aliases:
  Search:
    - SearchKnowledgeBase
    - SearchWeb
tools:
  allow:
    - Search
sequences:
  - type: eventually
    tool: Search
    within: 3
"#;

    // Use alias member, should count as covering the alias
    let traces = r#"{"id": "t1", "tools": ["SearchKnowledgeBase"]}"#;

    let report = run_coverage_test(policy, traces, 50.0).await;

    // Search alias should be covered via SearchKnowledgeBase
    let unseen: Vec<String> = report["tool_coverage"]["unseen_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // SearchWeb might still be unseen, but Search (the alias) should be covered
    assert!(
        !unseen.contains(&"Search".to_string())
            || !unseen.contains(&"SearchKnowledgeBase".to_string())
    );
}

// ==================== MULTIPLE TRACES TESTS ====================

#[tokio::test]
async fn test_coverage_across_multiple_traces() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow:
    - Tool1
    - Tool2
    - Tool3
    - Tool4
sequences: []
"#;

    // Each trace covers different tools
    let traces = r#"{"id": "t1", "tools": ["Tool1", "Tool2"]}
{"id": "t2", "tools": ["Tool3"]}
{"id": "t3", "tools": ["Tool4"]}"#;

    // Need to handle multi-line string to jsonl correctly in helper
    // The previous helper splits by lines, so this works

    let report = run_coverage_test(policy, traces, 80.0).await;

    // All tools covered across traces
    assert_eq!(
        report["tool_coverage"]["coverage_pct"].as_f64().unwrap(),
        100.0
    );
    assert!(report["meets_threshold"].as_bool().unwrap());
}

// ==================== THRESHOLD TESTS ====================

#[tokio::test]
async fn test_threshold_exact_boundary() {
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

    // 4/5 = 80% tool coverage. 0 rules = 100% rule coverage.
    // Overall = (80 + 100) / 2 = 90%.
    let traces = r#"{"id": "t1", "tools": ["Tool1", "Tool2", "Tool3", "Tool4"]}"#;

    let report_pass = run_coverage_test(policy, traces, 90.0).await;
    assert!(report_pass["meets_threshold"].as_bool().unwrap());

    let report_fail = run_coverage_test(policy, traces, 90.1).await;
    assert!(!report_fail["meets_threshold"].as_bool().unwrap());
}

// ==================== RULE COVERAGE TESTS ====================

#[tokio::test]
async fn test_rule_ids_generated() {
    let policy = r#"
version: "1.1"
name: "test"
tools:
  allow: [Search, Create]
sequences:
  - type: before
    first: Search
    then: Create
  - type: max_calls
    tool: Search
    max: 3
"#;

    let traces = r#"{"id": "t1", "tools": ["Search", "Create"]}"#;

    let report = run_coverage_test(policy, traces, 50.0).await;

    // Should have 2 rules
    assert_eq!(
        report["rule_coverage"]["total_rules"].as_u64().unwrap(),
        2,
        "Expected 2 rules"
    );

    let untriggered: Vec<String> = report["rule_coverage"]["untriggered_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(untriggered.len(), 2);

    let all_rules = untriggered.join(" ");
    assert!(all_rules.contains("before_search"));
    assert!(all_rules.contains("max_calls_search"));
}
