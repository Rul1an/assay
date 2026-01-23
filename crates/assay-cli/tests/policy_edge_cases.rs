use assay_core::mcp::policy::McpPolicy;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_validation_fails_unknown_rule_ref() {
    // Tests that validates() catches references to non-existent rules in kill_switch
    let yaml = r#"
version: "2.0"
runtime_monitor:
  enabled: true
  rules:
    - id: "rule-1"
      type: file_open
      match: { path_globs: ["*"] }
kill_switch:
  enabled: true
  triggers:
    - on_rule: "non-existent-rule"
      mode: immediate
"#;
    let policy: McpPolicy = serde_yaml::from_str(yaml).expect("Deserialization should succeed");

    // Explicitly call validate() as from_file() would
    let val = policy.validate();
    assert!(val.is_err(), "Validation should fail for unknown rule ID");
    let err = val.unwrap_err().to_string();
    assert!(
        err.contains("unknown rule id"),
        "Error message should mention unknown rule id: {}",
        err
    );
}

#[test]
fn test_unknown_fields_are_ignored_via_loader() {
    // Tests that McpPolicy::from_file uses serde_ignored to warn instead of crash
    let mut tmp = NamedTempFile::new().unwrap();
    write!(
        tmp,
        r#"
version: "2.0"
random_toplevel_field: "junk"
discovery:
  enabled: true
  weird_discovery_setting: 123
"#
    )
    .unwrap();

    // Should load successfully despite unknown fields
    let policy = McpPolicy::from_file(tmp.path()).expect("Should load policy with unknown fields");

    // Verify valid data checks out
    assert!(policy.discovery.is_some());
    assert!(policy.discovery.unwrap().enabled);
}

#[test]
fn test_partial_policy_defaults() {
    // Test that omitted optional blocks result in None/Default
    let yaml = r#"
version: "2.0"
"#;
    let policy: McpPolicy = serde_yaml::from_str(yaml).unwrap();
    assert!(policy.discovery.is_none());
    assert!(policy.runtime_monitor.is_none());
    assert!(policy.kill_switch.is_none());
}
