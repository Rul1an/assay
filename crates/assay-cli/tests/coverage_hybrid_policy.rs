//! Adding an empty MCP schema section must not erase declarations from a coverage report.
use serde_json::Value;
use std::process::Command;

#[test]
fn coverage_keeps_the_policy_declarations_when_schemas_are_present() {
    let dir = tempfile::tempdir().expect("create fixture");
    std::fs::write(
        dir.path().join("trace.jsonl"),
        "{\"type\":\"call_tool\",\"tool_name\":\"read_file\",\"arguments\":{\"path\":\"/example\"}}\n",
    )
    .unwrap();
    for (field, rules, tools) in [
        ("sequences: [{type: require, tool: read_file}]", 1, 1),
        ("aliases: {reader: [read_file]}", 0, 2),
        ("tools: {!key allow: [read_file]}", 0, 1),
        (
            "tools: {allow: [read_file]}\nsequences: null\naliases: null\non_error: null",
            0,
            1,
        ),
        ("tools: {require_args: {read_file: [path]}}", 0, 1),
    ] {
        for extra in ["", "schemas: {}\n"] {
            std::fs::write(
                dir.path().join("policy.yaml"),
                format!("version: 2\n{field}\n{extra}"),
            )
            .unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_assay"))
                .current_dir(dir.path())
                .args([
                    "coverage",
                    "--policy",
                    "policy.yaml",
                    "--trace-file",
                    "trace.jsonl",
                    "--format",
                    "json",
                ])
                .output()
                .expect("run assay coverage");
            let report: Value = serde_json::from_slice(&output.stdout).expect("coverage report");
            assert_eq!(
                report["rule_coverage"]["total_rules"], rules,
                "{field} {extra}"
            );
            assert_eq!(
                report["tool_coverage"]["total_tools_in_policy"], tools,
                "{field} {extra}"
            );
            assert_eq!(
                report["tool_coverage"]["tools_seen_in_traces"], 1,
                "{field} {extra}"
            );
            assert!(
                output.status.success(),
                "{field} {extra}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn coverage_checks_the_policy_byte_limit_before_parsing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("trace.jsonl"), "").unwrap();
    let mut policy = b"version: 2\ntools: {allow: [read_file]}\n#".to_vec();
    policy.resize(1_000_000, b'x');
    for oversized in [false, true] {
        if oversized {
            policy.push(b'x');
        }
        std::fs::write(dir.path().join("policy.yaml"), &policy).unwrap();
        let run = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args([
                "coverage",
                "--policy",
                "policy.yaml",
                "--trace-file",
                "trace.jsonl",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        if oversized {
            assert!(
                !run.status.success(),
                "accepted a policy past the source limit"
            );
            assert!(
                String::from_utf8_lossy(&run.stderr).contains("source bytes"),
                "{:?}",
                run
            );
            assert!(run.stdout.is_empty());
        } else {
            assert!(run.status.success(), "{:?}", run);
        }
    }
}

#[test]
fn coverage_names_a_misspelled_tagged_field() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("policy.yaml"),
        "tools: {!key allowed: [read_file]}",
    )
    .unwrap();
    std::fs::write(dir.path().join("trace.jsonl"), "").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args([
            "coverage",
            "--policy",
            "policy.yaml",
            "--trace-file",
            "trace.jsonl",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("tools.allowed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn coverage_and_eval_policy_readers_agree_on_alias_key_identity() {
    use assay_core::{mcp::policy::McpPolicy, model::Policy};

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("policy.yaml");
    for field in ["aliases", "!key aliases"] {
        for (keys, accepted) in [
            ("{0x10: [Read], 16: [Write]}", true),
            ("{16: [Read], '16': [Write]}", false),
            ("{!tag local: [Read], local: [Write]}", false),
        ] {
            let yaml = format!("version: 2\n{field}: {keys}\n");
            std::fs::write(&file, &yaml).unwrap();
            let eval = Policy::load(&file);
            let coverage = McpPolicy::load_for_coverage(&yaml);
            assert_eq!(eval.is_ok(), accepted, "{yaml}");
            assert_eq!(coverage.is_ok(), accepted, "{yaml}");
            if accepted {
                assert_eq!(coverage.unwrap().1.aliases, eval.unwrap().aliases);
            }
        }
    }
}

#[test]
fn coverage_preserves_yaml_extension_values_outside_the_policy() {
    let yaml = "version: 2\nx-extension: {? [a, b]: value, tools: &tools [Read]}\ntools: {allow: *tools}\n";
    let (_, policy) = assay_core::mcp::policy::McpPolicy::load_for_coverage(yaml).unwrap();
    assert_eq!(policy.tools.allow.unwrap(), ["Read"]);
}

#[test]
fn coverage_rejects_duplicate_keys_at_their_respective_scope() {
    for entries in [
        "x-a: 1\nx-a: 2",
        "x-a: {0x10: x, 16: y}",
        "metadata: {x-inner: {16: a, '16': b}}",
    ] {
        let yaml = format!("version: 2\n{entries}");
        assert!(assay_core::mcp::policy::McpPolicy::load_for_coverage(&yaml).is_err());
    }
}
