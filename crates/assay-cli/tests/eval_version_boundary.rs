use std::{fs, process::Command};

#[test]
fn versionless_eval_is_read_only_by_migrate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    let body = "suite: conversion\nmodel: trace\ntests:\n- id: a\n  input: hello\n  expected: {type: must_contain, must_contain: [hello]}\n";
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args(args)
            .args(["--config", "eval.yaml"])
            .output()
            .unwrap()
    };

    fs::write(&path, body).unwrap();
    let refused = run(&["validate"]);
    assert_eq!(refused.status.code(), Some(2));
    let report = [refused.stdout, refused.stderr].concat();
    assert!(String::from_utf8_lossy(&report).contains("configVersion"));
    assert_eq!(run(&["migrate", "--check"]).status.code(), Some(2));
    assert_eq!(fs::read_to_string(&path).unwrap(), body);
    assert!(run(&["migrate"]).status.success());
    assert_eq!(
        fs::read_to_string(path.with_extension("yaml.bak")).unwrap(),
        body
    );
    assert!(run(&["validate"]).status.success());
    assert!(run(&["migrate", "--check"]).status.success());

    for key in ["version", "configVersion"] {
        fs::write(&path, format!("{key}: 1\n{body}")).unwrap();
        assert!(run(&["validate"]).status.success());
    }
}

#[test]
fn migrate_preserves_the_assertion_from_each_legacy_expected_shape() {
    for expected in [
        "{must_contain: [hello]}",
        "[{must_contain: [hello]}]",
        "[{type: must_contain, must_contain: [hello]}]",
        "{type: must_contain, must_contain: hello}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("eval.yaml");
        let source = format!("configVersion: 1\nsuite: conversion\nmodel: trace\ntests:\n- id: checked\n  input: hello\n  expected: {expected}\n");
        fs::write(&path, &source).unwrap();
        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_assay"))
                .args(args)
                .arg("--config")
                .arg(&path)
                .output()
                .unwrap()
        };
        let old = run(&["validate"]);
        assert_eq!(old.status.code(), Some(2), "{expected}");
        assert_eq!(run(&["migrate", "--check"]).status.code(), Some(2));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert!(run(&["migrate"]).status.success(), "{expected}");
        let document: serde_yaml::Value =
            serde_yaml::from_slice(&fs::read(&path).unwrap()).unwrap();
        let actual = serde_json::to_value(&document["tests"][0]["expected"]).unwrap();
        assert_eq!(
            actual,
            serde_json::json!({"type":"must_contain","must_contain":["hello"]})
        );
        assert!(run(&["validate"]).status.success());
        assert!(run(&["migrate", "--check"]).status.success());
    }
}

#[test]
fn external_policy_is_only_read_during_migration() {
    for (expected, policy) in [
        (
            "{type: args_valid, policy: shared.yaml}",
            "version: '2.0'\nschemas: {Search: {type: object}}",
        ),
        ("{type: sequence_valid, policy: shared.yaml}", "[Search]"),
        (
            "{type: '$ref', path: shared.yaml}",
            "{type: must_contain, must_contain: [hello]}",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("eval.yaml");
        let input = dir.path().join("shared.yaml");
        let source = format!("configVersion: 1\nsuite: external\nmodel: trace\ntests:\n- id: checked\n  input: hello\n  expected: {expected}\n");
        fs::write(&path, &source).unwrap();
        fs::write(&input, policy).unwrap();
        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args(args)
                .args(["--config", "eval.yaml"])
                .output()
                .unwrap()
        };
        let rejected = run(&["validate"]);
        assert_eq!(rejected.status.code(), Some(2), "{expected}");
        assert!(
            String::from_utf8_lossy(&[rejected.stdout, rejected.stderr].concat())
                .contains("assay migrate")
        );
        assert_eq!(run(&["migrate", "--check"]).status.code(), Some(2));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert!(run(&["migrate"]).status.success());
        fs::remove_file(input).unwrap();
        assert!(run(&["validate"]).status.success());
        assert!(run(&["migrate", "--check"]).status.success());
        assert_eq!(
            fs::read_to_string(path.with_extension("yaml.bak")).unwrap(),
            source
        );
    }
}

#[test]
fn tool_name_argument_maps_are_read_only_by_migrate() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("eval.yaml");
    let source = "configVersion: 1\nsuite: args\nmodel: trace\ntests:\n- id: search\n  input: hi\n  expected:\n    type: args_valid\n    schema: {Search: {type: object, required: [q]}}\n";
    fs::write(&config, source).unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(tmp.path())
            .args(args)
            .args(["--config", "eval.yaml"])
            .output()
            .unwrap()
    };
    let refused = cli(&["validate"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&[refused.stdout, refused.stderr].concat())
            .contains("assay migrate")
    );
    assert_eq!(cli(&["migrate", "--check"]).status.code(), Some(2));
    assert!(cli(&["migrate"]).status.success());
    assert_eq!(
        fs::read_to_string(config.with_extension("yaml.bak")).unwrap(),
        source
    );
    assert!(cli(&["validate"]).status.success());
    assert!(cli(&["migrate", "--check"]).status.success());
}
