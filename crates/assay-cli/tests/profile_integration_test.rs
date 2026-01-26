use assert_cmd::prelude::*;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_profile_cli_workflow() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let out_yaml = tmp.path().join("policy.yaml");
    let out_report = tmp.path().join("report.md");

    let mut cmd = Command::new(assert_cmd::cargo_bin!("assay"));
    cmd.arg("sandbox")
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

    let yaml_content = std::fs::read_to_string(out_yaml)?;
    assert!(yaml_content.contains("api_version: 1"));
    assert!(yaml_content.contains("extends:"));

    Ok(())
}

#[test]
fn test_profile_json_output() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let out_json = tmp.path().join("policy.json");

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
    let json_content = std::fs::read_to_string(out_json)?;
    assert!(json_content.contains("\"api_version\": 1"));

    Ok(())
}
