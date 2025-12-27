use assay_core::config::load_config;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_compat_yaml_anchors() -> anyhow::Result<()> {
    // Verify YAML anchor support
    let mut tmp = NamedTempFile::new()?;
    writeln!(tmp, r#"
version: 1
suite: anchors
model: dummy
settings: &default_settings
  timeout_seconds: 60
  cache: true

tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
    # No way to inject settings into test scope yet, but verifying parse succeeds
"#)?;

    let cfg = load_config(tmp.path(), false)?;
    assert_eq!(cfg.settings.timeout_seconds, Some(60));
    Ok(())
}

#[test]
fn test_compat_unknown_fields() -> anyhow::Result<()> {
    // Verify forward compatibility (ignoring unknown fields)
    let mut tmp = NamedTempFile::new()?;
    writeln!(tmp, r#"
version: 1
suite: unknown_fields
model: dummy
future_field: "should be ignored"
settings:
  new_setting_2026: true
tests:
  - id: t1
    input: {{ prompt: "hi", extra_input: "ignored" }}
    expected: {{ type: must_contain, must_contain: ["hi"], future_metric_param: 123 }}
"#)?;

    let cfg = load_config(tmp.path(), false)?;
    assert_eq!(cfg.suite, "unknown_fields");
    // Should pass without error
    Ok(())
}

#[test]
fn test_compat_duplicate_tools_sequence() -> anyhow::Result<()> {
    // Verify duplicate tools in sequence logic (legacy list)
    let mut tmp = NamedTempFile::new()?;
    writeln!(tmp, r#"
version: 1
suite: dupes
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected:
       type: sequence_valid
       sequence: ["tool_a", "tool_b", "tool_a"]
"#)?;

    let cfg = load_config(tmp.path(), false)?;
    // Use pattern match to verifying parsing
    if let assay_core::model::Expected::SequenceValid { sequence, .. } = &cfg.tests[0].expected {
        let seq = sequence.as_ref().unwrap();
        assert_eq!(seq.len(), 3);
        assert_eq!(seq[2], "tool_a");
    } else {
        panic!("Wrong variant");
    }
    Ok(())
}

#[test]
fn test_compat_mixed_inline_and_file() -> anyhow::Result<()> {
    // One test inline, one file (partially migrated config)
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("mixed.yaml");
    let policy_path = dir.path().join("policy.yaml");

    std::fs::write(&policy_path, "type: object")?;

    std::fs::write(
        &config_path,
        r#"
version: 1
suite: mixed
model: dummy
tests:
  - id: inline
    input: { prompt: "1" }
    expected:
      type: args_valid
      schema: { type: object }
  - id: file
    input: { prompt: "2" }
    expected:
      type: args_valid
      policy: policy.yaml
"#,
    )?;

    // Load with resolution enabled (via resolve_policies or check if loader does it?)
    // Loader calls normalize_paths but NOT resolve_policies automatically unless we use `verify` or `migrate`.
    // Wait, `load_config` does NOT call `resolve_policies`.
    // It calls `normalize_paths`.
    // The runner usually calls `resolve_policies`?
    // Let's check `runner.rs` or `main.rs`.
    // In `main.rs` -> dispatch -> run -> checks legacy.
    // It doesn't seem to call `resolve_policies` automatically for execution?
    // If so, external files rely on specific metrics handling them?
    // `ArgsValid` metric usually loads schema?
    // Let's check `crates/assay-metrics/src/args_valid.rs`.

    // BUT `load_config` should succeed parsing.
    let cfg = load_config(&config_path, false)?;
    assert_eq!(cfg.tests.len(), 2);
    Ok(())
}
