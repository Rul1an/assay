use serde_json::json;
use std::{fs, process::Command};

#[test]
fn migrate_resolves_nested_policy_paths_in_one_pass() {
    for (expected, filename, content) in [
        (
            json!({"type":"args_valid","policy":"rule.yaml"}),
            "rule.yaml",
            r#"{"Search":{"type":"object"}}"#,
        ),
        (
            json!({"type":"sequence_valid","policy":"rule.yaml"}),
            "rule.yaml",
            r#"["Search"]"#,
        ),
        (
            json!({"type":"json_schema","json_schema":"","schema_file":"answer.json"}),
            "answer.json",
            r#"{"type":"string"}"#,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let refs = project.join("checks");
        fs::create_dir_all(&refs).unwrap();
        let original = "suite: nested\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    expected: {$ref: checks/expected.yaml}\n";
        let config = project.join("eval.yaml");
        fs::write(&config, original).unwrap();
        fs::write(refs.join("expected.yaml"), expected.to_string()).unwrap();
        fs::write(refs.join(filename), content).unwrap();
        let migrate = |options: &[&str]| {
            let out = Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args(["migrate", "--config", "project/eval.yaml"])
                .args(options)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{expected}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        migrate(&[]);
        let migrated = fs::read_to_string(&config).unwrap();
        fs::remove_file(refs.join("expected.yaml")).unwrap();
        if filename == "rule.yaml" {
            fs::remove_file(refs.join(filename)).unwrap();
        } else {
            assert!(
                migrated.contains("schema_file: checks/answer.json"),
                "{migrated}"
            );
        }
        migrate(&["--check"]);
        migrate(&[]);
        assert_eq!(fs::read_to_string(&config).unwrap(), migrated);
        assert_eq!(
            fs::read_to_string(config.with_extension("yaml.bak")).unwrap(),
            original
        );
    }
}

#[test]
fn nested_expected_reference_is_still_refused_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let original = "suite: nested\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    expected: {$ref: second.yaml}\n";
    fs::write(dir.path().join("eval.yaml"), original).unwrap();
    fs::write(dir.path().join("second.yaml"), "{$ref: second.yaml}").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["migrate", "--config", "eval.yaml"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unresolved `$ref`"));
    assert_eq!(
        fs::read_to_string(dir.path().join("eval.yaml")).unwrap(),
        original
    );
    assert!(!dir.path().join("eval.yaml.bak").exists());
}

#[test]
fn schema_above_working_directory_cannot_be_replaced_by_a_local_decoy() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("a/project");
    let shared = tmp.path().join("shared");
    let decoy = dir.join("shared");
    for path in [&dir, &shared, &decoy] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(dir.join("eval.yaml"), "suite: nested\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    expected: {$ref: ../../shared/block.yaml}\n").unwrap();
    fs::write(
        shared.join("block.yaml"),
        r#"{"type":"json_schema","json_schema":"","schema_file":"answer.json"}"#,
    )
    .unwrap();
    fs::write(shared.join("answer.json"), r#"{"type":"string"}"#).unwrap();
    fs::write(decoy.join("answer.json"), "invalid json").unwrap();
    for options in [vec![], vec!["--check"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(&dir)
            .args(["migrate", "--config", "eval.yaml"])
            .args(options)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let result = fs::read_to_string(dir.join("eval.yaml")).unwrap();
    assert!(
        result.contains("schema_file: ../../shared/answer.json"),
        "{result}"
    );
    fs::write(
        dir.join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t","timestamp":0,"input":{"prompt":"hello"}}
{"type":"episode_end","episode_id":"t","timestamp":1,"final_output":"\"ok\""}
"#,
    )
    .unwrap();
    fs::write(decoy.join("answer.json"), r#"{"type":"number"}"#).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(&dir)
            .args([
                "run",
                "--config",
                "eval.yaml",
                "--trace-file",
                "trace.jsonl",
                "--replay-strict",
                "--no-cache",
                "--db",
                ":memory:",
            ])
            .output()
            .unwrap()
    };
    let pass = replay();
    assert!(
        pass.status.success(),
        "{}",
        String::from_utf8_lossy(&pass.stderr)
    );
    fs::write(shared.join("answer.json"), r#"{"type":"number"}"#).unwrap();
    assert_eq!(replay().status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn nested_schema_reached_through_a_link_and_parent_refuses_a_different_target() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let shared = tmp.path().join("shared");
    fs::create_dir(&project).unwrap();
    fs::create_dir_all(shared.join("child")).unwrap();
    std::os::unix::fs::symlink(shared.join("child"), project.join("link")).unwrap();
    let original = "suite: nested\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    expected: {$ref: link/../block.yaml}\n";
    fs::write(project.join("eval.yaml"), original).unwrap();
    fs::write(
        shared.join("block.yaml"),
        r#"{"type":"json_schema","json_schema":"","schema_file":"answer.json"}"#,
    )
    .unwrap();
    fs::write(shared.join("answer.json"), r#"{"type":"string"}"#).unwrap();
    fs::write(project.join("answer.json"), r#"{"type":"number"}"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(&project)
        .args(["migrate", "--config", "eval.yaml"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("schema_file"));
    assert_eq!(
        fs::read_to_string(project.join("eval.yaml")).unwrap(),
        original
    );
    assert!(!project.join("eval.yaml.bak").exists());
}

#[cfg(unix)]
#[test]
fn nested_schema_with_a_directory_suffix_does_not_become_a_readable_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let original = "suite: nested\nmodel: trace\ntests:\n  - id: t\n    input: hello\n    expected: {$ref: block.yaml}\n";
    fs::write(dir.join("eval.yaml"), original).unwrap();
    fs::write(
        dir.join("block.yaml"),
        r#"{"type":"json_schema","json_schema":"","schema_file":"answer/"}"#,
    )
    .unwrap();
    fs::write(dir.join("answer"), r#"{"type":"string"}"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .args(["migrate", "--config", "eval.yaml"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("schema_file"));
    assert_eq!(fs::read_to_string(dir.join("eval.yaml")).unwrap(), original);
    assert!(!dir.join("eval.yaml.bak").exists());
}

#[test]
fn migrate_inlines_the_effective_fields_of_a_mixed_policy() {
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
fn a_sequence_policy_is_rebased_only_inside_a_referenced_block() {
    for (nested, mixed) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let checks = project.join("checks");
        fs::create_dir_all(&checks).unwrap();
        let mut block = json!({"type":"sequence_valid","policy":"shared.json"});
        if mixed {
            block["rules"] = json!([{"type":"require","tool":"Inline"}]);
        }
        fs::write(checks.join("expected.json"), block.to_string()).unwrap();
        for (base, name) in [
            (dir.path(), "Caller"),
            (&project, "Project"),
            (&checks, "Nested"),
        ] {
            fs::write(base.join("shared.json"), json!([name]).to_string()).unwrap();
        }
        let expected = if nested {
            json!({"$ref":"checks/expected.json"})
        } else {
            block
        };
        let config = project.join("eval.yaml");
        fs::write(&config, json!({"configVersion":1,"suite":"mixed","model":"trace","tests":[{"id":"check","input":"hi","expected":expected}]}).to_string()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args(["migrate", "--config", "project/eval.yaml"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let doc: serde_json::Value = serde_yaml::from_slice(&fs::read(config).unwrap()).unwrap();
        let mut want =
            json!({"type":"sequence_valid","sequence":[if nested {"Nested"} else {"Caller"}]});
        if mixed {
            want["rules"] = json!([{"type":"require","tool":"Inline"}]);
        }
        assert_eq!(doc["tests"][0]["expected"], want);
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
