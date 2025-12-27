use assay_core::config::{load_config, resolve::resolve_policies};
use assay_core::model::Expected;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_equivalence_args_valid() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("legacy.yaml");
    let policy_path = dir.path().join("policy.yaml");

    std::fs::write(
        &policy_path,
        r#"
type: object
properties:
  foo: { type: string }
"#,
    )?;

    std::fs::write(
        &config_path,
        r#"
suite: equivalence
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
       type: args_valid
       policy: policy.yaml
"#,
    )?;

    // 1. Load Legacy
    let legacy = load_config(&config_path, true)?;
    assert!(legacy.is_legacy());
    assert_eq!(legacy.tests[0].expected.get_policy_path(), Some("policy.yaml"));

    // 2. Resolve (Migrate in memory)
    let migrated = resolve_policies(legacy, dir.path())?;

    // 3. Verify internal structure
    // Should have no policy path, but populated schema
    assert_eq!(migrated.tests[0].expected.get_policy_path(), None);

    if let Expected::ArgsValid { schema, .. } = &migrated.tests[0].expected {
        let s = schema.as_ref().expect("schema should be populated");
        assert_eq!(s["type"], "object");
        assert_eq!(s["properties"]["foo"]["type"], "string");
    } else {
        panic!("Expected ArgsValid variant");
    }

    Ok(())
}

#[test]
fn test_equivalence_sequence_valid_legacy_list() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("legacy_seq.yaml");
    let policy_path = dir.path().join("seq.yaml");

    // Legacy list format in policy file
    std::fs::write(
        &policy_path,
        r#"
- tool_a
- tool_b
"#,
    )?;

    std::fs::write(
        &config_path,
        r#"
suite: equivalence
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
       type: sequence_valid
       policy: seq.yaml
"#,
    )?;

    let legacy = load_config(&config_path, true)?;
    let migrated = resolve_policies(legacy, dir.path())?;

    assert_eq!(migrated.tests[0].expected.get_policy_path(), None);

    if let Expected::SequenceValid { sequence, .. } = &migrated.tests[0].expected {
        let seq = sequence.as_ref().expect("sequence should be populated");
        assert_eq!(seq, &vec!["tool_a", "tool_b"]);
    } else {
        panic!("Expected SequenceValid variant");
    }

    Ok(())
}

#[test]
fn test_equivalence_sequence_valid_dsl_rules() -> anyhow::Result<()> {
    // Test that we can also resolve a policy file containing DSL rules (intermediate state)
    let dir = tempdir()?;
    let config_path = dir.path().join("dsl_ext.yaml");
    let policy_path = dir.path().join("rules.yaml");

    std::fs::write(
        &policy_path,
        r#"
- type: require
  tool: tool_c
"#,
    )?;

    std::fs::write(
        &config_path,
        r#"
suite: equivalence
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
       type: sequence_valid
       policy: rules.yaml
"#,
    )?;

    // Note: resolve_policies attempts to parse as Vec<String> first, fails, then Vec<SequenceRule>.
    let legacy = load_config(&config_path, true)?;
    let migrated = resolve_policies(legacy, dir.path())?;

    assert_eq!(migrated.tests[0].expected.get_policy_path(), None);

    if let Expected::SequenceValid { rules, .. } = &migrated.tests[0].expected {
        let r = rules.as_ref().expect("rules should be populated");
        match &r[0] {
            assay_core::model::SequenceRule::Require { tool } => {
                assert_eq!(tool, "tool_c");
            }
            _ => panic!("wrong rule type"),
        }
    } else {
        panic!("Expected SequenceValid variant");
    }

    Ok(())
}
