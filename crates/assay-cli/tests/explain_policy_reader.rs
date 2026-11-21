use assay_core::model::Policy;
use assert_cmd::Command;
use std::fs;

#[test]
fn file_readers_report_the_same_unknown_policy_path() {
    let dir = tempfile::tempdir().unwrap();
    let policy = dir.path().join("policy.yaml");
    let trace = dir.path().join("trace.json");
    fs::write(&trace, "[]").unwrap();
    fs::write(&policy, "version: 2\ntools:\n  requires_args: {}\n").unwrap();

    let cli = Command::new(assert_cmd::cargo::cargo_bin!("assay"))
        .args(["explain", "--trace"])
        .arg(&trace)
        .arg("--policy")
        .arg(&policy)
        .assert()
        .code(2);
    let cli_error = String::from_utf8_lossy(&cli.get_output().stderr);
    let model_error = format!("{:#}", Policy::load(&policy).unwrap_err());
    assert!(
        cli_error.contains("tools.requires_args") && model_error.contains("tools.requires_args"),
        "CLI: {cli_error}\nPolicy::load: {model_error}"
    );
}

#[test]
fn both_file_readers_enforce_the_policy_byte_limit() {
    let dir = tempfile::tempdir().unwrap();
    let policy = dir.path().join("policy.yaml");
    let trace = dir.path().join("trace.json");
    fs::write(&trace, "[]").unwrap();
    let mut bytes = b"version: 2\n#".to_vec();
    bytes.resize(1_000_000, b'x');
    for (extra, exit) in [(false, 0), (true, 2)] {
        if extra {
            bytes.push(b'x');
        }
        fs::write(&policy, &bytes).unwrap();
        assert_eq!(Policy::load(&policy).is_ok(), !extra);
        Command::new(assert_cmd::cargo::cargo_bin!("assay"))
            .args(["explain", "--trace"])
            .arg(&trace)
            .arg("--policy")
            .arg(&policy)
            .assert()
            .code(exit);
    }
}

#[test]
fn file_policy_preserves_defaults_and_opaque_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    for text in [
        "!policy {version: 2}",
        r#"
version: 2
name: demo
metadata: {vendor-key: [true, 2]}
tools:
  allow: [Shell]
  deny: [Mail]
  require_args: {Shell: [command]}
  arg_constraints: {Shell: {command: {nested: [1, false]}}}
sequences: [{type: require, tool: Shell}]
aliases: {local: [Shell]}
on_error: allow
"#,
    ] {
        fs::write(&path, text).unwrap();
        let direct: Policy = serde_yaml::from_str(text).unwrap();
        assert_eq!(
            serde_json::to_value(Policy::load(&path).unwrap()).unwrap(),
            serde_json::to_value(direct).unwrap()
        );
    }
    for text in [
        "{}",
        "version: 2\nschemas: {}",
        "version: 2\nx-defs: {}",
        "[2]",
        "version: 2\ntools: [[]]",
        "version: 2\nsequences: null",
    ] {
        fs::write(&path, text).unwrap();
        assert!(Policy::load(&path).is_err(), "accepted {text}");
    }
}

#[test]
fn tagged_field_names_use_the_underlying_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    fs::write(&path, "!key version: 2\ntools: {!key allow: [Shell]}").unwrap();
    assert_eq!(Policy::load(&path).unwrap().tools.allow.unwrap(), ["Shell"]);
    fs::write(&path, "version: 2\ntools: {!key requires_args: {}}").unwrap();
    assert!(format!("{:#}", Policy::load(&path).unwrap_err()).contains("tools.requires_args"));
}

#[test]
fn duplicate_aliases_in_a_file_require_one_effective_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let text = "version: 2\naliases: {local: [Read], local: [Shell]}";
    fs::write(&path, text).unwrap();
    let direct: Policy = serde_yaml::from_str(text).unwrap();
    assert_eq!(direct.aliases["local"], ["Shell"]);
    assert!(format!("{:#}", Policy::load(&path).unwrap_err()).contains("duplicate"));
}

#[test]
fn eval_policy_preserves_string_key_identity_and_wide_string_scalars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    for text in [
        "version: 2\naliases: {0x10: [Read], 16: [Write]}",
        "version: 2\ntools: {require_args: {0x10: [first], 16: [second]}}",
        "version: 18446744073709551616",
    ] {
        let historic: Policy = serde_yaml::from_str(text).unwrap();
        fs::write(&path, text).unwrap();
        assert_eq!(
            serde_json::to_value(Policy::load(&path).unwrap()).unwrap(),
            serde_json::to_value(historic).unwrap(),
            "{text}"
        );
    }
}

#[test]
fn eval_policy_rejects_equivalent_plain_and_quoted_string_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    for text in [
        "version: 2\naliases: {16: [Read], '16': [Write]}",
        "version: 2\ntools: {require_args: {16: [first], '16': [second]}}",
    ] {
        fs::write(&path, text).unwrap();
        assert!(format!("{:#}", Policy::load(&path).unwrap_err()).contains("duplicate"));
    }
}

#[test]
fn positional_sequence_errors_keep_the_path_to_the_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    let policy = dir.path().join("policy.yaml");
    fs::write(
        &policy,
        "version: 2\nsequences: [[require, {tool: Read, args_match: {}, extraneous: true}]]",
    )
    .unwrap();
    let diagnostic = format!("{:#}", Policy::load(&policy).unwrap_err());
    assert!(
        diagnostic.contains("sequences.0.1.extraneous"),
        "{diagnostic}"
    );
}
