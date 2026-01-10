#[cfg(test)]
mod tests {
    use crate::discovery::config_files::scan_config_files;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_claude_config() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join(".config/claude");
        fs::create_dir_all(&config_dir).unwrap();

        let config_path = config_dir.join("claude_desktop_config.json");
        let content = r#"{
            "mcpServers": {
                "filesystem": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                },
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {
                        "GITHUB_TOKEN": "ghp_secret"
                    }
                }
            }
        }"#;
        fs::write(&config_path, content).unwrap();

        let servers = scan_config_files(vec![config_path]);

        assert_eq!(servers.len(), 2);

        let filesystem = servers.iter().find(|s| s.name == Some("filesystem".to_string())).unwrap();
        assert_eq!(filesystem.id, "claude_desktop-filesystem");
        // Check auth status
        assert!(matches!(filesystem.auth, crate::discovery::types::AuthStatus::None));

        let github = servers.iter().find(|s| s.name == Some("github".to_string())).unwrap();
        // Check auth status inferred from env
        assert!(matches!(github.auth, crate::discovery::types::AuthStatus::ApiKey));
        assert!(github.env_vars.contains(&"GITHUB_TOKEN".to_string()));
    }
}
