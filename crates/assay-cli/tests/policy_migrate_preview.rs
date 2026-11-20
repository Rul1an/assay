use std::{fs, process::Command};

#[test]
fn check_previews_the_migration_without_writing_either_path() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("policy.yaml");
    let output = dir.path().join("out.yaml");
    let original = r#"# retain the original bytes
version: "1.0"
allow: [Search]
tools: {allow: [Read]}
schemas: {Read: {type: object}}
blocklist: [Retired]
x-source: {team: audit}
constraints:
  - tool: Search
    params: {query: {matches: "^public/"}}
"#;
    fs::write(&input, original).unwrap();
    fs::write(&output, "existing output").unwrap();

    let run = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .env_remove("ASSAY_STRICT_DEPRECATIONS")
            .args(["policy", "migrate", "--input", "policy.yaml"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    let preview = run(&["--dry-run"]);
    let migrated: serde_yaml::Value = serde_yaml::from_slice(&preview).unwrap();
    assert_eq!(migrated["blocklist"][0].as_str(), Some("Retired"));
    assert_eq!(migrated["tools"]["allow"][0].as_str(), Some("Read"));
    assert_eq!(migrated["tools"]["allow"][1].as_str(), Some("Search"));
    assert_eq!(migrated["x-source"]["team"].as_str(), Some("audit"));
    assert_eq!(
        migrated["schemas"]["Search"]["properties"]["query"]["pattern"].as_str(),
        Some("^public/")
    );
    assert_eq!(migrated["schemas"]["Read"]["type"].as_str(), Some("object"));
    assert_eq!(migrated["version"].as_str(), Some("2.0"));
    for legacy in ["allow", "deny", "constraints"] {
        assert!(migrated.get(legacy).is_none(), "obsolete key {legacy}");
    }
    for args in [&["--check"][..], &["--check", "--output", "out.yaml"]] {
        let checked = run(args);
        assert_eq!(fs::read_to_string(&input).unwrap(), original);
        assert_eq!(fs::read_to_string(&output).unwrap(), "existing output");
        assert_eq!(checked, preview);
    }
    run(&["--output", "out.yaml"]);
    assert_eq!(fs::read_to_string(&input).unwrap(), original);
    assert_eq!(
        fs::read_to_string(&output).unwrap().trim_end(),
        String::from_utf8(preview).unwrap().trim_end()
    );
}

#[test]
fn migrate_inlines_the_effective_fields_of_a_mixed_policy() {
    use serde_json::json;
    let inline_schema = json!({"version":"2.0","schemas":{"Inline":{"type":"string"}}});
    let inline_rules = json!([{"type":"require","tool":"Inline"}]);
    for (fields, file, canonical) in [
        (
            json!({"type":"args_valid","schema":inline_schema}),
            json!({"version":"2.0","deny":["File"]}),
            json!({"type":"args_valid","schema":inline_schema}),
        ),
        (
            json!({"type":"sequence_valid","sequence":["Inline"]}),
            json!(["File"]),
            json!({"type":"sequence_valid","sequence":["Inline"]}),
        ),
        (
            json!({"type":"sequence_valid","rules":inline_rules}),
            json!(["File"]),
            json!({"type":"sequence_valid","sequence":["File"],"rules":inline_rules}),
        ),
        (
            json!({"type":"sequence_valid","sequence":["Inline"]}),
            json!({"version":"2.0","sequences":inline_rules}),
            json!({"type":"sequence_valid","sequence":["Inline"],"rules":inline_rules}),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("eval.yaml");
        let mut mixed = fields;
        mixed["policy"] = json!("shared.json");
        let source = json!({"configVersion":1,"suite":"mixed","model":"trace","tests":[{"id":"mixed","input":"hello","expected":mixed}]});
        fs::write(&config, source.to_string()).unwrap();
        fs::write(dir.path().join("shared.json"), file.to_string()).unwrap();
        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args(args)
                .args(["--config", "eval.yaml"])
                .output()
                .unwrap()
        };
        assert_eq!(
            run(&["migrate", "--check"]).status.code(),
            Some(2),
            "{mixed}"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), source.to_string());
        assert!(run(&["migrate"]).status.success(), "{mixed}");
        let output: serde_yaml::Value =
            serde_yaml::from_slice(&fs::read(&config).unwrap()).unwrap();
        let expected = serde_json::to_value(&output["tests"][0]["expected"]).unwrap();
        assert_eq!(expected, canonical);
        fs::remove_file(dir.path().join("shared.json")).unwrap();
        assert!(run(&["validate"]).status.success());
        assert!(run(&["migrate", "--check"]).status.success());
    }
}

#[test]
fn migrate_refuses_an_ineffective_file_rule_even_when_shadowed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("eval.yaml");
    let source = "configVersion: 1\nsuite: mixed\nmodel: trace\ntests:\n- id: shadowed\n  input: hello\n  expected: {type: sequence_valid, policy: shared.yaml, rules: [{type: require, tool: Inline}]}\n";
    fs::write(&input, source).unwrap();
    fs::write(
        dir.path().join("shared.yaml"),
        "[{type: before, first: A, then: A}]",
    )
    .unwrap();
    for options in [vec!["--check"], vec![]] {
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args(["migrate", "--config", "eval.yaml"])
            .args(options)
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && error.contains("identical tools"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&input).unwrap(), source);
        assert!(!input.with_extension("yaml.bak").exists());
    }
}

#[test]
fn sequence_migration_uses_the_same_file_as_run() {
    for inline in ["", ", rules: [{type: max_calls, tool: Caller, max: 1}]"] {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(dir.path().join("policy.yaml"), "[]").unwrap();
        fs::write(project.join("policy.yaml"), "[Project]").unwrap();
        let config = project.join("eval.yaml");
        fs::write(&config, format!("configVersion: 1\nsuite: mixed\nmodel: trace\ntests:\n- id: t1\n  input: hi\n  expected: {{type: sequence_valid, policy: policy.yaml{inline}}}\n")).unwrap();
        fs::write(dir.path().join("trace.jsonl"), concat!(
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}"#, "\n",
        r#"{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"hello"}"#, "\n"
    )).unwrap();
        let execute = |args: &[&str]| {
            let output = Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args(args)
                .args(["--config", "project/eval.yaml"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        let run = [
            "run",
            "--trace-file",
            "trace.jsonl",
            "--no-cache",
            "--db",
            ":memory:",
        ];
        execute(&run);
        execute(&["migrate"]);
        execute(&run);
    }
}

#[test]
fn file_only_args_migration_cannot_replace_an_invalid_runtime_policy() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(dir.path().join("args.yaml"), "broken: [").unwrap();
    fs::write(project.join("args.yaml"), "version: '2.0'\ndeny: [Other]\n").unwrap();
    let config = project.join("eval.yaml");
    let source = "configVersion: 1\nsuite: paths\nmodel: trace\ntests:\n- id: check\n  input: hi\n  expected: {type: args_valid, policy: args.yaml}\n";
    fs::write(&config, source).unwrap();
    let migrate = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["migrate", "--config", "project/eval.yaml"])
        .output()
        .unwrap();
    assert_eq!(migrate.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&migrate.stderr).contains("args.yaml"));
    assert_eq!(fs::read_to_string(&config).unwrap(), source);
    assert!(!config.with_extension("yaml.bak").exists());
}

#[test]
fn file_only_migration_keeps_a_config_relative_source_when_the_caller_has_none() {
    for (kind, file, field) in [
        (
            "args_valid",
            r#"{"version":"2.0","schemas":{"Search":{"type":"object"}}}"#,
            "schema",
        ),
        ("sequence_valid", r#"["Search"]"#, "sequence"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("policy.yaml"), file).unwrap();
        let config = project.join("eval.yaml");
        let source = format!("suite: paths\nmodel: trace\ntests:\n- id: check\n  input: hi\n  expected: {{type: {kind}, policy: policy.yaml}}\n");
        fs::write(&config, &source).unwrap();
        let migrate = || {
            Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args(["migrate", "--config", "project/eval.yaml"])
                .output()
                .unwrap()
        };
        #[cfg(unix)]
        {
            let link = dir.path().join("policy.yaml");
            std::os::unix::fs::symlink("missing.yaml", &link).unwrap();
            assert_eq!(migrate().status.code(), Some(2), "{kind}: dangling policy");
            assert_eq!(fs::read_to_string(&config).unwrap(), source);
            assert!(!config.with_extension("yaml.bak").exists());
            fs::remove_file(link).unwrap();
        }
        let out = migrate();
        assert!(
            out.status.success(),
            "{kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let doc: serde_json::Value = serde_yaml::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(
            doc["tests"][0]["expected"][field],
            serde_json::from_str::<serde_json::Value>(file).unwrap()
        );
        assert_eq!(
            fs::read_to_string(config.with_extension("yaml.bak")).unwrap(),
            source
        );
    }
}
