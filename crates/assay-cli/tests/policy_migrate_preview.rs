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
fn fmt_preserves_current_policy_fields_and_refuses_legacy_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("policy.yaml");
    let output = dir.path().join("formatted.yaml");
    let original =
        "version: '2.0'\ntools: {allow: [Read]}\nblocklist: [Retired]\nx-origin: example\n";
    fs::write(&input, original).unwrap();
    let invoke = |path: &std::path::Path, dest: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["policy", "fmt", "--input"])
            .arg(path)
            .arg("--output")
            .arg(dest)
            .output()
            .unwrap()
    };
    let first = invoke(&input, &output);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let formatted = fs::read(&output).unwrap();
    assert_eq!(
        serde_yaml::from_slice::<serde_yaml::Value>(&formatted).unwrap(),
        serde_yaml::from_str::<serde_yaml::Value>(original).unwrap()
    );
    let validated = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["policy", "validate", "--input"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let second = invoke(&output, &output);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read(&output).unwrap(), formatted);

    let old = "version: '1.0'\nallow: [Read]\n";
    fs::write(&input, old).unwrap();
    assert!(!invoke(&input, &output).status.success());
    assert_eq!(fs::read_to_string(&input).unwrap(), old);
    assert_eq!(fs::read(&output).unwrap(), formatted);
}

#[test]
fn strict_mode_allows_explicit_migration_and_refuses_legacy_validation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let legacy = "version: '1.0'\nallow: [Read]\n";
    fs::write(&path, legacy).unwrap();
    let run = |command, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .env("ASSAY_STRICT_DEPRECATIONS", "1")
            .args(["policy", command, "--input"])
            .arg(&path)
            .args(extra)
            .output()
            .unwrap()
    };
    assert!(!run("validate", &[]).status.success());
    let preview = run("migrate", &["--check"]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    let write = run("migrate", &[]);
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap().trim_end(),
        String::from_utf8(preview.stdout).unwrap().trim_end()
    );
    assert!(run("validate", &[]).status.success());
}

#[test]
fn coverage_migration_accepts_numeric_text_without_a_coverage_only_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let raw = "version: 2\nname: 2.50\nallow: [Read]\ntools: {deny: [0x10]}\n";
    fs::write(&path, raw).unwrap();
    let preview = |flags: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["policy", "migrate", "--check", "--input"])
            .arg(&path)
            .args(flags)
            .output()
            .unwrap()
    };
    assert!(!preview(&[]).status.success());
    let result = preview(&["--coverage"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let out = String::from_utf8(result.stdout).unwrap();
    let (_, policy) = assay_core::mcp::policy::McpPolicy::load_for_coverage(&out).unwrap();
    assert_eq!(policy.version, "2");
    assert_eq!(policy.name, "2.50");
    assert_eq!(policy.tools.allow.unwrap(), ["Read"]);
    assert_eq!(policy.tools.deny.unwrap(), ["0x10"]);
    assert_eq!(fs::read_to_string(path).unwrap(), raw);
}

#[test]
fn coverage_preview_matches_written_bytes_when_alias_names_need_restoring() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("old.yaml");
    let output = dir.path().join("converted.yaml");
    fs::write(
        &input,
        "version: '1.0'\naliases: {C: [Write], A: [2.50], B: [Read]}\n",
    )
    .unwrap();
    let invoke = |check| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
        command
            .args(["policy", "migrate", "--coverage", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output);
        if check {
            command.arg("--check");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    let preview = invoke(true);
    assert!(!output.exists());
    invoke(false);
    assert_eq!(fs::read(&output).unwrap(), preview);
    let tree: serde_yaml::Value = serde_yaml::from_slice(&preview).unwrap();
    let keys: Vec<_> = tree["aliases"]
        .as_mapping()
        .unwrap()
        .keys()
        .map(|key| key.as_str().unwrap())
        .collect();
    assert_eq!(keys, ["A", "B", "C"]);
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
        execute(&["migrate"]);
        execute(&run);
    }
}

#[test]
fn coverage_migration_refuses_unreadable_file_sizes_before_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let limit = assay_core::model::MAX_POLICY_INPUT_BYTES as usize;
    // A long comment vanishes on conversion, so only the input limit can reject it.
    // A long metadata string survives, so rewriting the tools can exceed the output limit.
    for (prefix, suffix, size, error) in [
        ("allow: [Read]\n#", "\n", limit, None),
        (
            "allow: [Read]\n#",
            "\n",
            limit + 1,
            Some("source bytes limit"),
        ),
        (
            "version: '1.0'\nallow: [Read]\nmetadata: {label: ",
            "}\n",
            limit,
            Some("converted policy exceeds"),
        ),
    ] {
        let source = format!(
            "{prefix}{}{suffix}",
            "a".repeat(size - prefix.len() - suffix.len())
        );
        fs::write(&path, &source).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["policy", "migrate", "--coverage", "--input"])
            .arg(&path)
            .output()
            .unwrap();
        if let Some(error) = error {
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains(error));
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        } else {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(fs::metadata(&path).unwrap().len() < limit as u64);
        }
    }
}
