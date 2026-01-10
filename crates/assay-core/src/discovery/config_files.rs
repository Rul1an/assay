use super::types::{DiscoveredServer, DiscoverySource, ServerStatus, Transport, PolicyStatus, AuthStatus};
use std::path::PathBuf;
use std::collections::HashMap;
use serde::Deserialize;

#[derive(Deserialize)]
struct ClaudeDesktopConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: Option<HashMap<String, ClaudeServerConfig>>,
}

#[derive(Deserialize)]
struct ClaudeServerConfig {
    command: String,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
}

pub fn scan_config_files(search_paths: Vec<PathBuf>) -> Vec<DiscoveredServer> {
    let mut servers = Vec::new();

    for path in search_paths {
        if !path.exists() {
            continue;
        }

        // Determine client type mainly by filename/path heuristics
        let client_type = if path.to_string_lossy().contains("claude_desktop_config.json") {
            "claude_desktop"
        } else if path.to_string_lossy().contains("cursor") {
             "cursor" // Cursor often uses same format as Claude
        } else {
            "generic"
        };

        // Try parsing as Claude-style config (most common standard)
        if let Ok(content) = std::fs::read_to_string(&path) {
             if let Ok(config) = serde_json::from_str::<ClaudeDesktopConfig>(&content) {
                 if let Some(mcp_servers) = config.mcp_servers {
                     for (name, srv) in mcp_servers {
                         let has_key = srv.env.as_ref().map(|e|
                            e.keys().any(|k| k.to_uppercase().contains("KEY") || k.to_uppercase().contains("TOKEN"))
                         ).unwrap_or(false);

                         let env_keys = srv.env.as_ref().map(|e| e.keys().cloned().collect()).unwrap_or_default();

                         servers.push(DiscoveredServer {
                             id: format!("{}-{}", client_type, name),
                             name: Some(name),
                             source: DiscoverySource::ConfigFile {
                                 path: path.clone(),
                                 client: client_type.to_string(),
                             },
                             transport: Transport::Stdio {
                                 command: srv.command,
                                 args: srv.args.unwrap_or_default(),
                             },
                             status: ServerStatus::Configured,
                             policy_status: PolicyStatus::Unmanaged, // Default until we check policies
                             auth: if has_key { AuthStatus::ApiKey } else { AuthStatus::None },
                             env_vars: env_keys,
                             risk_hints: vec![],
                         });
                     }
                 }
             }
        }
    }
    servers
}
