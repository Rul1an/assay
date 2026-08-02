use assay_core::config::{load_config, resolve::resolve_policies};
use assay_core::model::Expected;
use tempfile::tempdir;

#[test]
fn test_equivalence_args_valid() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("legacy.yaml");
    let policy_path = dir.path().join("policy.yaml");

    std::fs::write(
        &policy_path,
        r#"
Search:
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
    let legacy = load_config(&config_path, true, false)?;
    assert!(legacy.is_legacy());
    assert_eq!(
        legacy.tests[0].expected.get_policy_path(),
        Some("policy.yaml")
    );

    // 2. Resolve (Migrate in memory)
    let migrated = resolve_policies(legacy, dir.path())?;

    // 3. Verify internal structure
    // Should have no policy path, but populated schema
    assert_eq!(migrated.tests[0].expected.get_policy_path(), None);

    if let Expected::ArgsValid { schema, .. } = &migrated.tests[0].expected {
        let s = schema.as_ref().expect("schema should be populated");
        assert_eq!(s["Search"]["type"], "object");
        assert_eq!(s["Search"]["properties"]["foo"]["type"], "string");
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

    let legacy = load_config(&config_path, true, false)?;
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

    let legacy = load_config(&config_path, true, false)?;
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

#[test]
fn test_equivalence_sequence_valid_structured_policy() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("structured.yaml");
    std::fs::write(
        dir.path().join("policy.yaml"),
        r#"
version: "1"
sequences:
  - type: require
    tool: Search
"#,
    )?;
    std::fs::write(
        &config_path,
        r#"
suite: equivalence
model: dummy
tests:
  - id: t1
    input: "hi"
    expected:
      type: sequence_valid
      policy: policy.yaml
"#,
    )?;

    let config = load_config(&config_path, true, false)?;
    let migrated = resolve_policies(config, dir.path())?;
    assert!(matches!(
        migrated.tests[0].expected,
        Expected::SequenceValid {
            policy: None,
            rules: Some(ref rules),
            ..
        } if rules.len() == 1
    ));
    Ok(())
}

fn write_ref_config(policy: &str) -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf)> {
    let dir = tempdir()?;
    std::fs::write(dir.path().join("policy.yaml"), policy)?;
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
suite: reference-resolution
model: dummy
tests:
  - id: referenced
    input: "hi"
    expected:
      $ref: policy.yaml
"#,
    )?;
    Ok((dir, config_path))
}

#[test]
fn test_reference_resolution_rejects_malformed_must_contain() -> anyhow::Result<()> {
    let (dir, config_path) = write_ref_config("must_contain: 42\n")?;
    let config = load_config(&config_path, true, false)?;

    let err = resolve_policies(config, dir.path())
        .expect_err("a malformed referenced assertion must not become an empty default");
    assert!(err.to_string().contains("must_contain"), "{err:#}");
    Ok(())
}

#[test]
fn test_reference_resolution_rejects_vacuous_must_contain() -> anyhow::Result<()> {
    let (dir, config_path) = write_ref_config("must_contain: []\n")?;
    let config = load_config(&config_path, true, false)?;

    let err = resolve_policies(config, dir.path())
        .expect_err("a referenced assertion must pass the same vacuity checks as inline YAML");
    assert!(err.to_string().contains("asserts nothing"), "{err:#}");
    Ok(())
}

#[test]
fn test_reference_resolution_accepts_strict_expected_but_rejects_root_schema() -> anyhow::Result<()>
{
    let (tagged, tagged_path) = write_ref_config("type: regex_match\npattern: '^ready$'\n")?;
    let tagged_config = load_config(&tagged_path, true, false)?;
    let tagged_resolved = resolve_policies(tagged_config, tagged.path())?;
    assert!(matches!(
        tagged_resolved.tests[0].expected,
        Expected::RegexMatch { .. }
    ));

    let (schema, schema_path) =
        write_ref_config("type: object\nproperties:\n  query: {type: string}\n")?;
    let schema_config = load_config(&schema_path, true, false)?;
    let schema_err = resolve_policies(schema_config, schema.path())
        .expect_err("a root schema has no tool name and cannot become an args_valid schema map");
    assert!(
        schema_err.to_string().contains("tool-name-to-schema map"),
        "{schema_err:#}"
    );

    let object_only = tempdir()?;
    std::fs::write(
        object_only.path().join("root-schema.yaml"),
        "properties:\n  query: {type: string}\n",
    )?;
    let object_only_path = object_only.path().join("eval.yaml");
    std::fs::write(
        &object_only_path,
        r#"
suite: object-only-root
model: dummy
tests:
  - id: root
    input: "hi"
    expected:
      type: args_valid
      policy: root-schema.yaml
"#,
    )?;
    let object_only_config = load_config(&object_only_path, true, false)?;
    let object_only_err = resolve_policies(object_only_config, object_only.path())
        .expect_err("an object-only root schema must not be flattened as a tool map");
    let object_only_chain = format!("{object_only_err:#}");
    assert!(
        object_only_chain.contains("tool-name-to-schema map"),
        "{object_only_chain}"
    );
    Ok(())
}

#[test]
fn referenced_schema_file_resolves_from_the_reference_directory() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let checks = dir.path().join("checks");
    std::fs::create_dir(&checks)?;
    std::fs::write(
        checks.join("response.schema.json"),
        r#"{"type":"object","required":["ok"]}"#,
    )?;
    std::fs::write(
        checks.join("expected.yaml"),
        "type: json_schema\njson_schema: ''\nschema_file: response.schema.json\n",
    )?;
    let config_path = dir.path().join("eval.yaml");
    std::fs::write(
        &config_path,
        r#"
suite: nested-reference
model: dummy
tests:
  - id: nested
    input: "hi"
    expected:
      $ref: checks/expected.yaml
"#,
    )?;

    let config = load_config(&config_path, true, false)?;
    let resolved = resolve_policies(config, dir.path())?;
    let Expected::JsonSchema { schema_file, .. } = &resolved.tests[0].expected else {
        panic!("reference must resolve to json_schema");
    };
    assert_eq!(
        schema_file.as_deref(),
        Some(
            checks
                .join("response.schema.json")
                .to_string_lossy()
                .as_ref()
        )
    );
    Ok(())
}

#[test]
fn policy_resolution_preserves_omitted_expected_for_migration() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
suite: migration
model: dummy
tests:
  - id: assertions-only
    input: "hi"
    assertions:
      - type: trace_max_steps
        max: 1
"#,
    )?;

    let config = load_config(&config_path, true, false)?;
    resolve_policies(config, dir.path())
        .expect("migration must preserve the compatibility sentinel for omitted expected");
    Ok(())
}

#[test]
fn test_policy_resolution_rejects_vacuous_or_unimplemented_constraints() -> anyhow::Result<()> {
    let args = tempdir()?;
    std::fs::write(args.path().join("schema.yaml"), "{}\n")?;
    let args_config_path = args.path().join("args.yaml");
    std::fs::write(
        &args_config_path,
        r#"
suite: policy-resolution
model: dummy
tests:
  - id: empty-schema
    input: "hi"
    expected:
      type: args_valid
      policy: schema.yaml
"#,
    )?;
    let args_config = load_config(&args_config_path, true, false)?;
    let args_err = resolve_policies(args_config, args.path())
        .expect_err("resolved policy files must pass the vacuity check");
    let args_chain = format!("{args_err:#}");
    assert!(args_chain.contains("asserts nothing"), "{args_chain}");

    let sequence = tempdir()?;
    std::fs::write(
        sequence.path().join("rules.yaml"),
        "- type: eventually\n  tool: Search\n  within: 2\n",
    )?;
    let sequence_config_path = sequence.path().join("sequence.yaml");
    std::fs::write(
        &sequence_config_path,
        r#"
suite: policy-resolution
model: dummy
tests:
  - id: unsupported-rule
    input: "hi"
    expected:
      type: sequence_valid
      policy: rules.yaml
"#,
    )?;
    let sequence_config = load_config(&sequence_config_path, true, false)?;
    let sequence_err = resolve_policies(sequence_config, sequence.path())
        .expect_err("resolved policy rules ignored by the evaluator must not execute");
    let sequence_chain = format!("{sequence_err:#}");
    assert!(
        sequence_chain.contains("not executable"),
        "{sequence_chain}"
    );
    Ok(())
}

#[test]
fn policy_resolution_refuses_to_flatten_structured_args_policy() -> anyhow::Result<()> {
    let dir = tempdir()?;
    std::fs::write(
        dir.path().join("policy.yaml"),
        r#"
version: "2.0"
deny: [Delete]
schemas:
  Search:
    type: object
    required: [query]
"#,
    )?;
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
suite: structured-policy
model: dummy
tests:
  - id: protected
    input: "hi"
    expected:
      type: args_valid
      policy: policy.yaml
"#,
    )?;

    let config = load_config(&config_path, true, false)?;
    let err = resolve_policies(config, dir.path())
        .expect_err("migration must not discard allow/deny/enforcement policy fields");
    assert!(
        err.to_string().contains("structured args_valid policy"),
        "{err:#}"
    );
    Ok(())
}
