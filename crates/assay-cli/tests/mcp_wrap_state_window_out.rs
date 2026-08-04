#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn read_json(path: &Path) -> Value {
    let content = std::fs::read_to_string(path).expect("state window report should exist");
    serde_json::from_str(&content).expect("state window report must be valid JSON")
}

#[test]
fn mcp_wrap_state_window_out_writes_valid_v1_report() -> anyhow::Result<()> {
    // `assay` is a bin of this package, so Cargo builds it and injects the path.
    let assay = env!("CARGO_BIN_EXE_assay");

    let tmp = TempDir::new()?;
    let policy_path = tmp.path().join("proxy-policy.yaml");
    let out = tmp.path().join("state-window.json");

    std::fs::write(
        &policy_path,
        r#"
version: "2.0"
name: "state-window-export"
tools:
  allow: ["*"]
enforcement:
  unconstrained_tools: allow
"#,
    )?;

    Command::new(assay)
        .args([
            "mcp",
            "wrap",
            "--policy",
            policy_path.to_string_lossy().as_ref(),
            "--event-source",
            "assay://tests/state-window",
            "--label",
            "default-mcp-server",
            "--state-window-out",
        ])
        .arg(&out)
        .arg("--")
        .arg(assay)
        .arg("--help")
        .assert()
        .success();

    let report = read_json(&out);
    assert_eq!(report["schema_version"], "session_state_window_v1");
    assert_eq!(report["report_version"], "1");
    assert_eq!(
        report["session"]["event_source"],
        "assay://tests/state-window"
    );
    assert_eq!(report["session"]["server_id"], "default-mcp-server");
    assert!(!report["session"]["session_id"]
        .as_str()
        .unwrap()
        .trim()
        .is_empty());
    assert_eq!(report["window"]["window_kind"], "session");
    assert_eq!(report["privacy"]["stores_raw_tool_args"], false);
    assert_eq!(report["privacy"]["stores_raw_prompt_bodies"], false);
    assert_eq!(report["privacy"]["stores_raw_document_bodies"], false);

    let id = report["snapshot"]["state_snapshot_id"].as_str().unwrap();
    assert!(id.starts_with("sha256:"));
    assert_eq!(id.len(), "sha256:".len() + 64);

    Ok(())
}
