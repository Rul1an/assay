use assay_mcp_server::cache::PolicyCaches;
use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::tools::{check_sequence, ToolContext};
use serde_json::json;

use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn run_check(policy_yaml: &str, history: Vec<&str>, next: &str) -> serde_json::Value {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir().join(format!("assay_evt_test_{}_{}", unique_id, count));
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

    let args = json!({
        "history": history,
        "next_tool": next,
        "policy": "policy.yaml"
    });

    let res = check_sequence::check_sequence(&ctx, &args).await.unwrap();
    let _ = tokio::fs::remove_dir_all(temp_dir).await;
    res
}

#[tokio::test]
async fn test_eventually_success() {
    let policy = r#"
version: "1.1"
name: "evt"
sequences:
  - type: eventually
    tool: Target
    within: 3
"#;
    // Index 0 -> OK
    let res = run_check(policy, vec![], "Target").await;
    assert!(res["allowed"].as_bool().unwrap());

    // Index 1 -> OK
    let res = run_check(policy, vec!["A"], "Target").await;
    assert!(res["allowed"].as_bool().unwrap());

    // Index 2 -> OK (within 3 means 0, 1, 2)
    let res = run_check(policy, vec!["A", "B"], "Target").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_eventually_fail_too_late() {
    let policy = r#"
version: "1.1"
name: "evt"
sequences:
  - type: eventually
    tool: Target
    within: 3
"#;
    // Index 3 -> Fail (3 >= 3)
    let res = run_check(policy, vec!["A", "B", "C"], "Target").await;
    assert!(!res["allowed"].as_bool().unwrap());
    let msg = res["violations"][0]["message"].as_str().unwrap();
    assert!(msg.contains("appeared at index 3 but must appear within first 3 calls"));
}

#[tokio::test]
async fn test_eventually_fail_timeout() {
    let policy = r#"
version: "1.1"
name: "evt"
sequences:
  - type: eventually
    tool: Target
    within: 3
"#;
    // Length 4, not found -> Fail
    let res = run_check(policy, vec!["A", "B", "C"], "D").await;
    // Trace: A, B, C, D (len 4). Target not in A,B,C,D.
    assert!(!res["allowed"].as_bool().unwrap());
    let msg = res["violations"][0]["message"].as_str().unwrap();
    assert!(msg.contains("required within first 3 calls but not found"));
}

#[tokio::test]
async fn test_eventually_pending() {
    let policy = r#"
version: "1.1"
name: "evt"
sequences:
  - type: eventually
    tool: Target
    within: 3
"#;
    // Length 2, not found -> OK (still time)
    let res = run_check(policy, vec!["A"], "B").await;
    // Trace: A, B (len 2). Target missing. 2 <= 3. OK.
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_eventually_alias() {
    let policy = r#"
version: "1.1"
name: "evt"
aliases:
  Goal: ["Target", "Final"]
sequences:
  - type: eventually
    tool: Goal
    within: 2
"#;
    // Check alias 1
    let res = run_check(policy, vec!["A"], "Target").await; // Index 1. OK.
    assert!(res["allowed"].as_bool().unwrap());

    // Check alias 2
    let res = run_check(policy, vec!["A"], "Final").await; // Index 1. OK.
    assert!(res["allowed"].as_bool().unwrap());

    // Fail late
    let res = run_check(policy, vec!["A", "B"], "Target").await; // Index 2. Fail.
    assert!(!res["allowed"].as_bool().unwrap());
}
