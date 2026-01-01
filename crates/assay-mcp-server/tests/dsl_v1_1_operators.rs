use assay_mcp_server::cache::PolicyCaches;
use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::tools::{check_sequence, ToolContext};
use serde_json::json;

use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn run_check(policy_yaml: &str, history: Vec<&str>, next: &str) -> serde_json::Value {
    // Unique dir to avoid collisions
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir().join(format!("assay_ops_test_{}_{}", unique_id, count));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    let policy_path = temp_dir.join("policy.yaml");
    tokio::fs::write(&policy_path, policy_yaml).await.unwrap();

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

    let args = json!({
        "history": history,
        "next_tool": next,
        "policy": "policy.yaml"
    });

    let res = check_sequence::check_sequence(&ctx, &args).await.unwrap();
    let _ = tokio::fs::remove_dir_all(temp_dir).await; // cleanup
    res
}

// ==================== MAX_CALLS TESTS ====================

#[tokio::test]
async fn test_max_calls_pass_under_limit() {
    let policy = r#"
version: "1.1"
name: "max_calls_test"
sequences:
  - type: max_calls
    tool: API
    max: 3
"#;
    // Trace: API, API (history) + API (next) = 3 calls. <= 3. OK.
    let res = run_check(policy, vec!["API", "API"], "API").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_max_calls_fail_over_limit() {
    let policy = r#"
version: "1.1"
name: "max_calls_test"
sequences:
  - type: max_calls
    tool: API
    max: 2
"#;
    // Trace: API, API (history) + API (next) = 3 calls. > 2. FAIL.
    let res = run_check(policy, vec!["API", "API"], "API").await;
    assert!(!res["allowed"].as_bool().unwrap());
    assert_eq!(res["violations"][0]["rule_type"], "max_calls");
    assert_eq!(res["violations"][0]["context"]["max"], 2);
    assert_eq!(res["violations"][0]["context"]["actual"], 3);
}

// ==================== AFTER TESTS ====================

#[tokio::test]
async fn test_after_pass_immediate() {
    let policy = r#"
version: "1.1"
name: "after_test"
sequences:
  - type: after
    trigger: Create
    then: Audit
    within: 2
"#;
    // Trace: Create (0), Audit (1). Audit is within 2 calls of Create. OK.
    let res = run_check(policy, vec!["Create"], "Audit").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_after_pass_within_limit() {
    let policy = r#"
version: "1.1"
name: "after_test"
sequences:
  - type: after
    trigger: Create
    then: Audit
    within: 2
"#;
    // Trace: Create (0), X (1), Audit (2). 2 - 0 = 2. limit is 2. Wait, logic is: strictly within N steps?
    // Implementation: valid indices for 'then' are trigger_idx + 1 ..= trigger_idx + within.
    // If Create is at 0, within 2 means indices 1, 2.
    // X at 1, Audit at 2. Yes.
    let res = run_check(policy, vec!["Create", "X"], "Audit").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_after_fail_exceeded_deadline() {
    let policy = r#"
version: "1.1"
name: "after_test"
sequences:
  - type: after
    trigger: Create
    then: Audit
    within: 1
"#;
    // Trace: Create (0), X (1), Y (2).
    // Deadline for Create(0) with within:1 is index 1.
    // X is at 1. Not Audit.
    // Y is at 2. > 1. Fail?
    // Check pending logic: at index 1 (X), pending is (0, deadline=1).
    // X is not Audit. X is not trigger (assumed).
    // Next idx 2 (Y). 2 > 1. Should fail because deadline passed.

    // Note: check_sequence result depends on if Y is allowed.
    // Violation generated at index 2 because 2 > 1.
    let res = run_check(policy, vec!["Create", "X"], "Y").await;
    assert!(!res["allowed"].as_bool().unwrap());
    let msg = res["violations"][0]["message"].as_str().unwrap();
    assert!(msg.contains("required within 1 calls after 'Create'"));
}

// ==================== NEVER_AFTER TESTS ====================

#[tokio::test]
async fn test_never_after_pass() {
    let policy = r#"
version: "1.1"
name: "never_after_test"
sequences:
  - type: never_after
    trigger: Archive
    forbidden: Delete
"#;
    // Delete BEFORE Archive is fine.
    let res = run_check(policy, vec!["Delete"], "Archive").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_never_after_fail() {
    let policy = r#"
version: "1.1"
name: "never_after_test"
sequences:
  - type: never_after
    trigger: Archive
    forbidden: Delete
"#;
    // Archive (0), Delete (1). Fail at 1.
    let res = run_check(policy, vec!["Archive"], "Delete").await;
    assert!(!res["allowed"].as_bool().unwrap());
    assert_eq!(res["violations"][0]["rule_type"], "never_after");
}

// ==================== SEQUENCE TESTS ====================

#[tokio::test]
async fn test_sequence_strict_pass() {
    let policy = r#"
version: "1.1"
name: "seq_test"
sequences:
  - type: sequence
    tools: ["A", "B", "C"]
    strict: true
"#;
    // A, B, C -> OK
    // Note: checking C.
    let res = run_check(policy, vec!["A", "B"], "C").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_sequence_strict_fail_intervening() {
    let policy = r#"
version: "1.1"
name: "seq_test"
sequences:
  - type: sequence
    tools: ["A", "B", "C"]
    strict: true
"#;
    // A, X (fail here?), B, C
    // At X (index 1). Seq idx 1 expected B. Found X. Fail.
    let res = run_check(policy, vec!["A"], "X").await;
    assert!(!res["allowed"].as_bool().unwrap());
    assert_eq!(res["violations"][0]["constraint"], "sequence_strict");
}

#[tokio::test]
async fn test_sequence_non_strict_pass_with_gaps() {
    let policy = r#"
version: "1.1"
name: "seq_test"
sequences:
  - type: sequence
    tools: ["A", "B", "C"]
    strict: false
"#;
    // A, X, B, Y, C -> OK
    let res = run_check(policy, vec!["A", "X", "B", "Y"], "C").await;
    assert!(res["allowed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_sequence_non_strict_fail_order() {
    let policy = r#"
version: "1.1"
name: "seq_test"
sequences:
  - type: sequence
    tools: ["A", "B", "C"]
    strict: false
"#;
    // A, C (fail? C came before B).
    // Implementation checks: if we see C (future item) while expecting B.
    // Current seq_idx for B is 1. C is at future_seq_idx 2.
    // If next_tool is C. We check matches_any(C, targets[1]). No.
    // Check future: targets[2] matches C? Yes.
    // Violation: order violated.
    let res = run_check(policy, vec!["A"], "C").await;
    assert!(!res["allowed"].as_bool().unwrap());
    assert_eq!(res["violations"][0]["constraint"], "sequence_order");
}
