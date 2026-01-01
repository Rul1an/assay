use assay_mcp_server::cache::PolicyCaches;
use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::tools::{check_sequence, ToolContext};
use serde_json::json;

#[tokio::test]
async fn test_alias_resolution_require() {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("assay_alias_test_{}", unique_id));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    let policy_path = temp_dir.join("alias_policy.yaml");
    let policy_content = r#"
version: "1.1"
name: "alias-test"
aliases:
  Search: ["SearchWeb", "SearchKB"]
sequences:
  - type: require
    tool: Search
"#;
    tokio::fs::write(&policy_path, policy_content)
        .await
        .unwrap();

    let cfg = ServerConfig::default();
    let caches = PolicyCaches::new(100);
    // Use temp_dir as policy root
    let policy_root_canon = tokio::fs::canonicalize(&temp_dir).await.unwrap();
    let ctx = ToolContext {
        policy_root: temp_dir.clone(),
        policy_root_canon,
        cfg,
        caches,
    };

    // Case 1: SearchWeb (Alias Member 1) -> Allowed
    let args1 = json!({
        "history": ["SearchWeb"],
        "next_tool": "Next",
        "policy": "alias_policy.yaml"
    });
    let res1 = check_sequence::check_sequence(&ctx, &args1).await.unwrap();
    assert_eq!(
        res1["allowed"], true,
        "SearchWeb should satisfy Search requirement"
    );

    // Case 2: SearchKB (Alias Member 2) -> Allowed
    let args2 = json!({
        "history": ["SearchKB"],
        "next_tool": "Next",
        "policy": "alias_policy.yaml"
    });
    let res2 = check_sequence::check_sequence(&ctx, &args2).await.unwrap();
    assert_eq!(
        res2["allowed"], true,
        "SearchKB should satisfy Search requirement"
    );

    // Case 3: Other -> Blocked
    let args3 = json!({
        "history": ["Other"],
        "next_tool": "Next",
        "policy": "alias_policy.yaml"
    });
    let res3 = check_sequence::check_sequence(&ctx, &args3).await.unwrap();
    assert_eq!(
        res3["allowed"], false,
        "Missing required tool (alias group) should fail"
    );
    let msg = res3["violations"][0]["message"].as_str().unwrap();
    assert!(
        msg.contains("required tool 'Search' (aliases: [\"SearchWeb\", \"SearchKB\"]) not found")
    );

    // Cleanup
    let _ = tokio::fs::remove_dir_all(temp_dir).await;
}

#[tokio::test]
async fn test_alias_resolution_before() {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("assay_alias_before_{}", unique_id));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    let policy_path = temp_dir.join("before_policy.yaml");
    let policy_content = r#"
version: "1.1"
name: "alias-before"
aliases:
  Prepare: ["Init", "Setup"]
  Action: ["DoIt", "Run"]
sequences:
  - type: before
    first: Prepare
    then: Action
"#;
    tokio::fs::write(&policy_path, policy_content)
        .await
        .unwrap();

    let cfg = ServerConfig::default();
    let caches = PolicyCaches::new(100);
    let policy_root_canon = tokio::fs::canonicalize(&temp_dir).await.unwrap();
    let ctx = ToolContext {
        policy_root: temp_dir.clone(),
        policy_root_canon,
        cfg,
        caches,
    };

    // Case 1: Init before DoIt -> Allowed
    let args1 = json!({
        "history": ["Init", "DoIt"],
        "next_tool": "End",
        "policy": "before_policy.yaml"
    });
    let res1 = check_sequence::check_sequence(&ctx, &args1).await.unwrap();
    assert_eq!(res1["allowed"], true, "Init before DoIt should pass");

    // Case 2: Setup before Run -> Allowed
    let args2 = json!({
        "history": ["Setup", "Run"],
        "next_tool": "End",
        "policy": "before_policy.yaml"
    });
    let res2 = check_sequence::check_sequence(&ctx, &args2).await.unwrap();
    assert_eq!(res2["allowed"], true, "Setup before Run should pass");

    // Case 3: DoIt without Prepare -> Fail
    // 'before' rule in assay only checks order IF both present?
    // Wait, my implementation:
    // if let Some(t_idx) = then_idx { if let Some(f_idx) = first_idx { ... } else { FAIL } }
    // So if 'then' (Action) is present, 'first' (Prepare) MUST be present.
    let args3 = json!({
        "history": ["DoIt"],
        "next_tool": "End",
        "policy": "before_policy.yaml"
    });
    let res3 = check_sequence::check_sequence(&ctx, &args3).await.unwrap();
    assert_eq!(res3["allowed"], false, "Action without Prepare should fail");

    // Case 4: Run before Init -> Fail (Order)
    let args4 = json!({
        "history": ["Run", "Init"],
        "next_tool": "End",
        "policy": "before_policy.yaml"
    });
    let res4 = check_sequence::check_sequence(&ctx, &args4).await.unwrap();
    assert_eq!(res4["allowed"], false, "Action before Prepare should fail");

    // Cleanup
    let _ = tokio::fs::remove_dir_all(temp_dir).await;
}
