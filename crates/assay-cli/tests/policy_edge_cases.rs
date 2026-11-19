use assay_core::mcp::policy::McpPolicy;
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
fn file_loader_names_unknown_root_and_discovery_fields() {
    let file = NamedTempFile::new().unwrap();
    let valid = "version: '2.0'\ndiscovery:\n  enabled: true\n";
    std::fs::write(file.path(), valid).unwrap();
    let policy = McpPolicy::from_file(file.path()).unwrap();
    assert!(policy.discovery.unwrap().enabled);

    let invalid = format!("{valid}  weird_discovery_setting: 123\nrandom_toplevel_field: junk\n");
    std::fs::write(file.path(), invalid).unwrap();
    let error = McpPolicy::from_file(file.path()).unwrap_err().to_string();
    assert!(
        error.contains("discovery.weird_discovery_setting"),
        "{error}"
    );
    assert!(error.contains("random_toplevel_field"), "{error}");
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
