//! Config Path Detection & Integration Helper
//!
//! Detects MCP client config locations and generates ready-to-use configurations.
//! Supports: Claude Desktop, Cursor, and generic MCP clients.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;

/// Supported MCP clients
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum McpClient {
    Claude,
    Cursor,
}

impl McpClient {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-desktop" | "claude_desktop" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Desktop",
            Self::Cursor => "Cursor",
        }
    }
}

/// Result of config path detection
#[derive(Debug)]
#[allow(dead_code)]
pub struct ConfigDetection {
    pub client: McpClient,
    pub config_path: PathBuf,
    pub exists: bool,
    pub current_config: Option<Value>,
}

/// Generated MCP server configuration
#[derive(Debug, Serialize)]
pub struct GeneratedConfig {
    pub server_name: String,
    pub config: McpServerEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}

// ============================================================================
// Path Detection
// ============================================================================

/// Detect the config file path for a given MCP client
pub fn detect_config_path(client: McpClient) -> Option<PathBuf> {
    match client {
        McpClient::Claude => detect_claude_config_path(),
        McpClient::Cursor => detect_cursor_config_path(),
    }
}

fn detect_claude_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|h| h.join("Library/Application Support/Claude/claude_desktop_config.json"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| d.join("Claude/claude_desktop_config.json"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|c| c.join("Claude/claude_desktop_config.json"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

fn detect_cursor_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| {
            h.join("Library/Application Support/Cursor/User/globalStorage/cursor.mcp/config.json")
        })
    }

    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| d.join("Cursor/User/globalStorage/cursor.mcp/config.json"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|c| c.join("Cursor/User/globalStorage/cursor.mcp/config.json"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// Full detection with config file reading
pub fn detect_config(client: McpClient) -> Option<ConfigDetection> {
    let config_path = detect_config_path(client)?;
    let exists = config_path.exists();

    let current_config = if exists {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };

    Some(ConfigDetection {
        client,
        config_path,
        exists,
        current_config,
    })
}

// ============================================================================
// Config Generation
// ============================================================================

/// Generate an MCP server config entry for Assay wrapper
pub fn generate_assay_config(
    server_name: &str,
    policy_path: &str,
    wrapped_command: &str,
    wrapped_args: &[String],
    assay_binary: Option<&str>,
) -> GeneratedConfig {
    // Detect assay binary location
    let assay_cmd = assay_binary
        .map(String::from)
        .or_else(detect_assay_binary)
        .unwrap_or_else(|| "assay".to_string());

    // Build args: mcp wrap --policy <path> -- <command> <args...>
    let mut args = vec![
        "mcp".to_string(),
        "wrap".to_string(),
        "--policy".to_string(),
        policy_path.to_string(),
        "--".to_string(),
        wrapped_command.to_string(),
    ];
    args.extend(wrapped_args.iter().cloned());

    GeneratedConfig {
        server_name: server_name.to_string(),
        config: McpServerEntry {
            command: assay_cmd,
            args,
            env: None,
        },
    }
}

/// Generate a filesystem server config (common use case)
pub fn generate_filesystem_config(
    policy_path: &str,
    allowed_directory: &str,
    assay_binary: Option<&str>,
) -> GeneratedConfig {
    generate_assay_config(
        "filesystem-secure",
        policy_path,
        "npx",
        &[
            "-y".to_string(),
            "@modelcontextprotocol/server-filesystem".to_string(),
            allowed_directory.to_string(),
        ],
        assay_binary,
    )
}

/// Try to find the assay binary
fn detect_assay_binary() -> Option<String> {
    // 1. Check if we're running as assay (use current exe)
    if let Ok(exe) = env::current_exe() {
        if exe
            .file_name()
            .map(|n| n.to_string_lossy().contains("assay"))
            .unwrap_or(false)
        {
            return Some(exe.to_string_lossy().to_string());
        }
    }

    // 2. Check common install locations
    let candidates = [
        dirs::home_dir().map(|h| h.join(".cargo/bin/assay")),
        dirs::home_dir().map(|h| h.join(".local/bin/assay")),
        Some(PathBuf::from("/usr/local/bin/assay")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    None
}

/// Get default policy path
pub fn default_policy_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("assay/policy.yaml")
}

// ============================================================================
// JSON Output Formatting
// ============================================================================

/// Format the generated config as a JSON snippet for mcpServers
pub fn format_as_mcp_servers_entry(config: &GeneratedConfig) -> String {
    let entry = json!({
        &config.server_name: &config.config
    });

    serde_json::to_string_pretty(&entry).unwrap_or_else(|_| "{}".to_string())
}

// ============================================================================
// CLI Output Helper
// ============================================================================

/// Generate the full CLI output for `assay mcp config-path`
pub fn generate_cli_output(
    client: McpClient,
    policy_path: Option<&str>,
    wrapped_server: Option<(&str, &[String])>,
) -> String {
    let detection = detect_config(client);
    let policy = policy_path
        .map(String::from)
        .unwrap_or_else(|| default_policy_path().to_string_lossy().to_string());

    let home_dir = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string());

    // Generate config
    let config = if let Some((cmd, args)) = wrapped_server {
        generate_assay_config("mcp-secure", &policy, cmd, args, None)
    } else {
        generate_filesystem_config(&policy, &home_dir, None)
    };

    let mut output = String::new();

    // Header
    output.push_str(&format!("┌─ {} Configuration\n", client.display_name()));
    output.push_str("│\n");

    // Config path status
    if let Some(ref det) = detection {
        output.push_str(&format!("│  Config file: {}\n", det.config_path.display()));
        if det.exists {
            output.push_str("│  Status: ✓ Found\n");
        } else {
            output.push_str("│  Status: ✗ Not found (will be created)\n");
        }
    } else {
        output.push_str("│  Config file: Could not detect path\n");
        output.push_str("│  Status: ✗ Unknown OS or client not installed\n");
    }

    output.push_str("│\n");
    output.push_str("├─ Policy file\n");
    output.push_str("│\n");
    output.push_str(&format!("│  {}\n", policy));
    output.push_str("│\n");

    // Generated config
    output.push_str("├─ Add this to your mcpServers:\n");
    output.push_str("│\n");

    let json_snippet = format_as_mcp_servers_entry(&config);
    for line in json_snippet.lines() {
        output.push_str(&format!("│  {}\n", line));
    }

    output.push_str("│\n");
    output.push_str("└─ Next steps:\n");
    output.push_str("   1. Create your policy file\n");
    output.push_str("   2. Add the above JSON to your config file\n");
    output.push_str(&format!("   3. Restart {}\n", client.display_name()));

    output
}

// ============================================================================
// Tests
// ============================================================================

pub fn run(args: crate::cli::args::ConfigPathArgs) {
    let client = match McpClient::from_str(&args.client) {
        Some(c) => c,
        None => {
            eprintln!(
                "Error: Unknown client '{}'. Supported: claude, cursor",
                args.client
            );
            std::process::exit(1);
        }
    };

    let wrapped_tuple = args.server.as_deref().and_then(|server_cmd| {
        let mut parts = server_cmd.split_whitespace();
        let cmd = parts.next()?;
        // Collect remaining parts as args
        let args: Vec<String> = parts.map(String::from).collect();
        Some((cmd.to_string(), args))
    });

    // We need a lifetime-bound slice for the tuple if we want to pass it around as (&str, &[String])
    // But since we own the strings inside the loop/closure, it's tricky to return references to them easily
    // without keeping the owner alive.
    // Let's just restructure the logic to avoid the complex double-parsing.

    // Easier approach: hold the owned vector
    let (server_cmd_owned, server_args_owned) = match wrapped_tuple {
        Some((cmd, args)) => (Some(cmd), args),
        None => (None, vec![]),
    };

    let wrapped_tuple_ref = server_cmd_owned
        .as_deref()
        .map(|cmd| (cmd, server_args_owned.as_slice()));

    if args.json {
        let detection = detect_config(client);
        let policy = args
            .policy
            .clone()
            .unwrap_or_else(|| default_policy_path().to_string_lossy().to_string());

        // Use home dir for allowlist if no wrapped server specified
        let home_dir = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string());

        let config = if let Some((cmd, args)) = wrapped_tuple_ref {
            generate_assay_config("mcp-secure", &policy, cmd, args, None)
        } else {
            generate_filesystem_config(&policy, &home_dir, None)
        };

        let output = json!({
            "client": client.display_name(),
            "config_path": detection.as_ref().map(|d| d.config_path.clone()),
            "config_exists": detection.as_ref().map(|d| d.exists).unwrap_or(false),
            "generated_server": config
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!(
            "{}",
            generate_cli_output(client, args.policy.as_deref(), wrapped_tuple_ref)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_from_str() {
        assert_eq!(McpClient::from_str("claude"), Some(McpClient::Claude));
        assert_eq!(McpClient::from_str("Claude"), Some(McpClient::Claude));
        assert_eq!(
            McpClient::from_str("claude-desktop"),
            Some(McpClient::Claude)
        );
        assert_eq!(McpClient::from_str("cursor"), Some(McpClient::Cursor));
        assert_eq!(McpClient::from_str("vscode"), None);
    }

    #[test]
    fn test_generate_filesystem_config() {
        let config = generate_filesystem_config(
            "/home/user/.config/assay/policy.yaml",
            "/home/user",
            Some("/usr/local/bin/assay"),
        );

        assert_eq!(config.server_name, "filesystem-secure");
        assert_eq!(config.config.command, "/usr/local/bin/assay");
        assert!(config.config.args.contains(&"mcp".to_string()));
        assert!(config.config.args.contains(&"wrap".to_string()));
        assert!(config.config.args.contains(&"--policy".to_string()));
    }

    #[test]
    fn test_format_as_mcp_servers_entry() {
        let config = GeneratedConfig {
            server_name: "test-server".to_string(),
            config: McpServerEntry {
                command: "assay".to_string(),
                args: vec!["mcp".to_string(), "wrap".to_string()],
                env: None,
            },
        };

        let output = format_as_mcp_servers_entry(&config);
        assert!(output.contains("test-server"));
        assert!(output.contains("assay"));
    }
}
