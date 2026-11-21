use assay_core::config::load_config;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_compat_yaml_anchors() -> anyhow::Result<()> {
    // Verify YAML anchor support
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
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
"#
    )?;

    let cfg = load_config(tmp.path(), false, false)?;
    assert_eq!(cfg.settings.timeout_seconds, Some(60));
    Ok(())
}

#[test]
fn test_strict_schema_rejection() -> anyhow::Result<()> {
    // Verify strict schema enforcement (unknown fields rejected)
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
version: 1
suite: unknown_fields
model: dummy
future_field: "should be rejected"
settings:
  new_setting_2026: true
tests:
  - id: t1
    input: {{ prompt: "hi", extra_input: "rejected" }}
    expected: {{ type: must_contain, must_contain: ["hi"], future_metric_param: 123 }}
"#
    )?;

    let res = load_config(tmp.path(), false, false);
    assert!(
        res.is_err(),
        "Schema should be strict and reject future_field"
    );
    let err = res.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("unknown field"),
        "Error should mention unknown field: {}",
        err_str
    );
    Ok(())
}

#[test]
fn test_compat_duplicate_tools_sequence() -> anyhow::Result<()> {
    // Verify duplicate tools in sequence logic (legacy list)
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
version: 1
suite: dupes
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected:
       type: sequence_valid
       sequence: ["tool_a", "tool_b", "tool_a"]
"#
    )?;

    let cfg = load_config(tmp.path(), false, false)?;
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
    let cfg = load_config(&config_path, false, false)?;
    assert_eq!(cfg.tests.len(), 2);
    Ok(())
}

#[test]
fn metric_threshold_typos_stop_execution_and_migration() {
    use assay_core::config::load_config_for_migration;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    for metric in [
        "type: semantic_similarity_to, semantic_similarity_to: hello",
        "type: faithfulness",
        "type: relevance",
    ] {
        let input = format!(
            "configVersion: 1\nsuite: thresholds\nmodel: trace\nsettings: {{thresholding: {{mode: relative}}}}\ntests:\n  - id: check\n    input: {{prompt: hello}}\n    expected: {{{metric}, thresholding: {{extra_limit: 0.25}}}}\n"
        );
        std::fs::write(&path, input).unwrap();
        for result in [
            load_config(&path, false, true)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            load_config_for_migration(&path)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        ] {
            let error = result.expect_err(metric);
            assert!(
                error.contains("expected.thresholding") && error.contains("extra_limit"),
                "{metric}: {error}"
            );
        }
    }
}

#[test]
fn metric_threshold_legacy_list_preserves_the_authored_value() -> anyhow::Result<()> {
    for value in ["{max_drop: 0.25}", "[0.25]"] {
        let mut file = NamedTempFile::new()?;
        writeln!(
            file,
            "configVersion: 1\nsuite: thresholds\nmodel: trace\nsettings: {{thresholding: {{mode: relative}}}}\ntests:\n  - id: check\n    input: hello\n    expected: [{{type: relevance, thresholding: {value}}}]"
        )?;
        let runtime = load_config(file.path(), false, true)?;
        let migration = assay_core::config::load_config_for_migration(file.path())?;
        for config in [runtime, migration.config] {
            assert_eq!(
                config.tests[0]
                    .expected
                    .thresholding_for_metric("relevance")
                    .and_then(|t| t.max_drop),
                Some(0.25)
            );
        }
    }
    Ok(())
}

#[test]
fn inline_expected_refuses_duplicate_members_before_validation() -> anyhow::Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(file, "version: 1\nsuite: duplicate\nmodel: trace\ntests:\n  - id: check\n    input: hello\n    expected: {{type: must_contain, must_contain: [yes], must_contain: [no]}}")?;
    let error = load_config(file.path(), false, true).expect_err("duplicate assertion key");
    assert!(error.to_string().contains("duplicate member"), "{error}");
    Ok(())
}

#[test]
fn referenced_expected_checks_duplicate_members_and_size() -> anyhow::Result<()> {
    use assay_core::config::{load_config_for_migration, resolve::resolve_authored_policies};
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("eval.yaml");
    let reference = dir.path().join("metric.yaml");
    std::fs::write(&config, "version: 1\nsuite: ref\nmodel: trace\ntests:\n  - id: check\n    input: hello\n    expected: {$ref: metric.yaml}\n")?;
    let resolve = || {
        let loaded = load_config_for_migration(&config)?;
        resolve_authored_policies(loaded.config, &config)
    };
    let mut valid = String::from("type: must_contain\nmust_contain: [hello]\n#");
    valid.push_str(&"x".repeat(assay_core::model::MAX_POLICY_INPUT_BYTES as usize - valid.len()));
    std::fs::write(&reference, &valid)?;
    resolve()?;
    for (source, reason) in [
        (
            "must_contain: [a]\nmust_contain: [b]".to_owned(),
            "duplicate member",
        ),
        (format!("{valid}x"), "source bytes limit"),
    ] {
        std::fs::write(&reference, source)?;
        let error = resolve().expect_err(reason);
        let message = format!("{error:#}");
        assert!(
            message.contains("metric.yaml") && message.contains(reason),
            "{message}"
        );
    }
    Ok(())
}

#[test]
fn authored_assertions_do_not_decode_json_from_reserved_string_fields() -> anyhow::Result<()> {
    use assay_core::config::{load_config_for_migration, resolve::resolve_authored_policies};
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("eval.yaml");
    let source = |expected: &str| {
        format!(
        "version: 1\nsuite: reserved\nmodel: trace\ntests:\n  - id: check\n    input: hello\n    expected: {expected}\n"
    )
    };
    // A schema property value is authored data, even when its key resembles Serde internals.
    let assertion = r#"{schema: {Search: {properties: {label: {$serde_json::private::RawValue: '{"type":"string"}'}}}}}"#;
    std::fs::write(&config, source(assertion))?;
    let error = load_config(&config, false, true).expect_err("reserved raw field");
    assert!(error.to_string().contains("reserved"), "{error}");
    std::fs::write(dir.path().join("metric.yaml"), assertion)?;
    std::fs::write(&config, source("{$ref: metric.yaml}"))?;
    let input = load_config_for_migration(&config)?;
    let error = resolve_authored_policies(input.config, &config).expect_err("reserved raw field");
    assert!(format!("{error:#}").contains("reserved"), "{error:#}");
    Ok(())
}

#[test]
fn eval_metadata_preserves_open_fields_and_refuses_duplicate_members() -> anyhow::Result<()> {
    use assay_core::config::load_config_for_migration;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("eval.yaml");
    let write = |metadata: &str| {
        std::fs::write(&path, format!(
            "configVersion: 1\nsuite: metadata\nmodel: trace\ntests:\n  - id: example\n    input: hello\n    expected: {{type: must_contain, must_contain: [hi]}}\n    {metadata}\n"
        ))
    };
    write("metadata: {team: [{label: blue, rank: 7}]}")?;
    let expected = serde_json::json!({"team": [{"label": "blue", "rank": 7}]});
    for config in [
        load_config(&path, false, true)?,
        load_config_for_migration(&path)?.config,
    ] {
        assert_eq!(config.tests[0].metadata.as_ref(), Some(&expected));
    }
    for empty in ["", "metadata: null"] {
        write(empty)?;
        for config in [
            load_config(&path, false, true)?,
            load_config_for_migration(&path)?.config,
        ] {
            assert!(config.tests[0].metadata.is_none());
        }
    }
    for (metadata, reason) in [
        (
            "metadata: {team: [{label: blue, label: green}]}",
            "JSON object contains a duplicate member",
        ),
        (
            r#"metadata: {user: {'$serde_json::private::RawValue': '{}'}}"#,
            "reserved Serde raw-value member",
        ),
    ] {
        write(metadata)?;
        let runtime =
            load_config(&path, false, true).expect_err("runtime must refuse ambiguous metadata");
        let migration = load_config_for_migration(&path)
            .err()
            .expect("migration must refuse ambiguous metadata");
        assert!(runtime.to_string().contains(reason), "{runtime}");
        assert!(migration.to_string().contains(reason), "{migration}");
    }
    Ok(())
}

#[test]
fn json_test_case_still_requires_the_last_positional_field() {
    use assay_core::model::TestCase;
    let fields =
        r#""example", "hello", {"type":"must_contain","must_contain":["hi"]}, null, null, []"#;
    let full: TestCase = serde_json::from_str(&format!("[{fields}, null]")).unwrap();
    assert!(full.metadata.is_none());
    let error = serde_json::from_str::<TestCase>(&format!("[{fields}]")).unwrap_err();
    assert!(error.to_string().contains("invalid length 6"), "{error}");
}

#[test]
fn assertion_payloads_refuse_repeated_members_without_closing_their_keys() -> anyhow::Result<()> {
    use assay_core::config::load_config_for_migration;
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("eval.yaml");
    for (pointer, template) in [
        ("/test_args/user", "{type: args_valid, tool: Search, test_args: {user: PAYLOAD}, policy: {type: object}}"),
        ("/policy/examples/0", "{type: args_valid, tool: Search, test_args: {}, policy: {type: object, examples: [PAYLOAD]}}"),
        ("/policy/user", "{type: tool_blocklist, test_tool_calls: [Search], policy: {blocked: [Archive], user: PAYLOAD}}"),
        ("/policy/user", "{type: sequence_valid, test_trace_raw: [{tool: a}], policy: {regex: '^a$', user: PAYLOAD}}"),
        ("/test_trace_raw/0/args", "{type: sequence_valid, test_trace_raw: [{tool: a, args: PAYLOAD}], policy: {regex: '^a$'}}"),
    ] {
        for repeated in [false, true] {
            let value = if repeated { "{label: blue, label: green}" } else { "{label: blue, count: 7}" };
            let assertion = template.replace("PAYLOAD", value);
            std::fs::write(&file, format!(
                "configVersion: 1\nsuite: labels\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    assertions: [{assertion}]\n"
            ))?;
            let runtime = load_config(&file, false, true);
            let migration = load_config_for_migration(&file);
            if repeated {
                let runtime = runtime.expect_err("duplicate in runtime assertion");
                let migration = migration.err().expect("duplicate in migration assertion");
                assert!(runtime.to_string().contains("duplicate member"), "{pointer}: {runtime}");
                assert!(migration.to_string().contains("duplicate member"), "{pointer}: {migration}");
            } else {
                for config in [runtime?, migration?.config] {
                    let assertions = serde_json::to_value(&config.tests[0].assertions)?;
                    assert_eq!(assertions[0].pointer(pointer), Some(&serde_json::json!({"label":"blue","count":7})));
                }
            }
        }
    }
    Ok(())
}
