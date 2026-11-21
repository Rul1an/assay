use assert_cmd::prelude::*;
use std::collections::BTreeSet;
use std::process::Command;
use tempfile::tempdir;

/// The keys `assay sandbox --policy` reads. A suggestion holds these and nothing else, so it loads
/// back without any block that reads as policy while nothing acts on it.
fn assert_holds_only_loader_keys(
    keys: BTreeSet<String>,
    fs: BTreeSet<String>,
    net: BTreeSet<String>,
) {
    let set = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(keys, set(&["api_version", "extends", "fs", "net"]));
    assert_eq!(fs, set(&["allow", "deny"]));
    assert_eq!(net, set(&["allow", "deny"]));
}

fn yaml_keys(value: &serde_yaml::Value) -> BTreeSet<String> {
    value
        .as_mapping()
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn json_keys(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(unix)]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[test]
fn test_profile_cli_workflow() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let out_yaml = tmp.path().join("policy.yaml");
    let out_report = tmp.path().join("report.md");
    let out_evidence = tmp.path().join("policy.evidence.yaml");

    let mut cmd = Command::new(assert_cmd::cargo_bin!("assay"));
    cmd.env("FOO_FEATURE", "1")
        .arg("sandbox")
        .arg("--profile")
        .arg(&out_yaml)
        .arg("--profile-report")
        .arg(&out_report)
        .arg("--quiet")
        .arg("--")
        .arg("true");

    let output = cmd.unwrap();
    assert!(output.status.success());

    // Verify files exist
    assert!(out_yaml.exists());
    assert!(out_report.exists());
    assert!(out_evidence.exists());

    let yaml_content = std::fs::read_to_string(out_yaml)?;
    let policy: serde_yaml::Value = serde_yaml::from_str(&yaml_content)?;
    assert_eq!(policy["api_version"], "assay/v1");
    assert!(
        policy["extends"].as_sequence().is_some_and(Vec::is_empty),
        "generated YAML policies must declare an empty extends list"
    );
    assert_holds_only_loader_keys(
        yaml_keys(&policy),
        yaml_keys(&policy["fs"]),
        yaml_keys(&policy["net"]),
    );
    assert!(
        !yaml_content.contains("FOO_FEATURE"),
        "environment variable names are an observation, not policy"
    );

    // The observations the suggestion no longer carries are in the report.
    let report = std::fs::read_to_string(out_report)?;
    assert!(
        report.contains("FOO_FEATURE"),
        "the report names the environment variables the command received:\n{report}"
    );

    let evidence_content = std::fs::read_to_string(out_evidence)?;
    assert!(
        evidence_content.contains("version: '1.0'")
            || evidence_content.contains("version: \"1.0\"")
    );
    assert!(evidence_content.contains("total_runs: 1"));
    assert!(
        !evidence_content.contains("sandbox_degradations:"),
        "healthy run should not emit sandbox degradation observations"
    );

    Ok(())
}

#[test]
fn test_profile_json_output() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let out_json = tmp.path().join("policy.json");
    let out_evidence = tmp.path().join("policy.evidence.json");

    let mut cmd = Command::new(assert_cmd::cargo_bin!("assay"));
    cmd.arg("sandbox")
        .arg("--profile")
        .arg(&out_json)
        .arg("--profile-format")
        .arg("json")
        .arg("--quiet")
        .arg("--")
        .arg("true");

    let output = cmd.unwrap();
    assert!(output.status.success());

    assert!(out_json.exists());
    assert!(out_evidence.exists());
    let json_content = std::fs::read_to_string(out_json)?;
    let policy: serde_json::Value = serde_json::from_str(&json_content)?;
    assert_eq!(policy["api_version"], "assay/v1");
    assert!(
        policy["extends"].as_array().is_some_and(Vec::is_empty),
        "generated JSON policies must declare an empty extends list"
    );
    assert_holds_only_loader_keys(
        json_keys(&policy),
        json_keys(&policy["fs"]),
        json_keys(&policy["net"]),
    );

    Ok(())
}

#[test]
#[cfg(unix)]
fn owasp_mcp05_sandbox_keeps_shell_metacharacters_as_argv() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let sentinel = tmp.path().join("injection-sentinel");
    let quoted_sentinel = shell_single_quote(&sentinel.to_string_lossy());
    let injection_arg = format!("; touch {quoted_sentinel}");

    let mut cmd = Command::new(assert_cmd::cargo_bin!("assay"));
    cmd.arg("sandbox")
        .arg("--quiet")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf safe")
        .arg(&injection_arg);

    let output = cmd.unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "safe");
    assert!(
        !sentinel.exists(),
        "sandbox command construction executed shell metacharacters from argv"
    );

    Ok(())
}

#[test]
fn profile_update_reports_unread_fields_before_rewriting() {
    let tmp = tempdir().unwrap();
    let trace = tmp.path().join("trace.jsonl");
    std::fs::write(
        &trace,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}"#,
    )
    .unwrap();
    let source = r#"{"version":"1.0","name":"example","created_at":"now","updated_at":"now","total_runs":0,
        "root_typo":true,"entries":{"files":{"/tmp/a":{"first_seen":1,"last_seen":1,"runs_seen":1,"entry_typo":false}}},
        "sandbox_degradations":[{"reason_code":"policy_conflict","degradation_mode":"audit_fallback","component":"landlock","observation_typo":true}]}"#;
    for name in ["profile.yaml", "profile.json"] {
        let path = tmp.path().join(name);
        std::fs::write(&path, source).unwrap();
        let result = Command::new(assert_cmd::cargo_bin!("assay"))
            .args(["profile", "update", "--profile"])
            .arg(&path)
            .args(["--input"])
            .arg(&trace)
            .args(["--run-id", "trial"])
            .output()
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes(), "{name}");
        assert_eq!(result.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&result.stderr);
        for field in [
            "root_typo",
            "entries.files./tmp/a.entry_typo",
            "sandbox_degradations.0.observation_typo",
        ] {
            assert!(stderr.contains(field), "{stderr}");
        }
    }
}

#[test]
fn profile_entry_keys_use_their_string_spelling_before_update() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("profile.yaml");
    let trace = tmp.path().join("trace.jsonl");
    std::fs::write(
        &trace,
        r#"{"type":"proc_exec","path":"/bin/a","timestamp":1}"#,
    )
    .unwrap();
    for (a, b, distinct) in [
        ("16", "0x10", true),
        ("\"16\"", "16", false),
        ("!key a", "a", false),
    ] {
        let source = format!("version: '1.0'\nname: 007\ncreated_at: now\nupdated_at: now\ntotal_runs: 1\nentries:\n  files:\n    {a}: {{first_seen: 7, last_seen: 7, runs_seen: 1}}\n    {b}: {{first_seen: 8, last_seen: 8, runs_seen: 1}}\n");
        std::fs::write(&path, &source).unwrap();
        let result = Command::new(assert_cmd::cargo_bin!("assay"))
            .args(["profile", "update", "--profile"])
            .arg(&path)
            .arg("--input")
            .arg(&trace)
            .args(["--run-id", "trial"])
            .output()
            .unwrap();
        if distinct {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let updated: serde_yaml::Value =
                serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(
                updated["entries"]["files"]["16"]["first_seen"].as_u64(),
                Some(7)
            );
            assert_eq!(
                updated["entries"]["files"]["0x10"]["first_seen"].as_u64(),
                Some(8)
            );
            assert_eq!(updated["name"].as_str(), Some("007"));
        } else {
            assert_eq!(result.status.code(), Some(2));
            assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
        }
    }
}
