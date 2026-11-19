use std::{fs, process::Command};

#[test]
fn check_previews_the_migration_without_writing_either_path() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("policy.yaml");
    let output = dir.path().join("out.yaml");
    let original = "# retain the original bytes\nversion: \"1.0\"\nallow: [Search]\n";
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
    assert!(!preview.is_empty());
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
