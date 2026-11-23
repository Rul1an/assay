//! Global Cursor MCP configuration is shared by setup and discovery.
#[test]
#[cfg(unix)]
fn cursor_setup_uses_the_global_mcp_file() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".cursor/mcp.json");
    std::fs::create_dir(config.parent().unwrap()).unwrap();
    std::fs::write(&config, r#"{"mcpServers":{}}"#).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["mcp", "config-path", "cursor", "--json"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["config_path"].as_str(), config.to_str());
    assert_eq!(value["config_exists"], true);
    let args = value["generated_server"]["config"]["args"]
        .as_array()
        .unwrap();
    assert_eq!(args.last().unwrap(), "<absolute-project-directory>");
    let text = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["mcp", "config-path", "cursor"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(String::from_utf8(text.stdout)
        .unwrap()
        .contains("<absolute-project-directory>"));
}
