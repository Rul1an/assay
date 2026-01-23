use crate::cli::args::DiscoverArgs;
use assay_core::discovery::{
    config_files::scan_config_files,
    processes::scan_processes,
    types::{HostInfo, Inventory, InventorySummary},
};
use std::path::PathBuf;
use sysinfo::System;

pub async fn run(args: DiscoverArgs) -> anyhow::Result<i32> {
    let policy_config = if let Some(path) = &args.policy {
        let p = assay_core::mcp::policy::McpPolicy::from_file(path)?;
        p.discovery
            .unwrap_or(assay_core::mcp::policy::DiscoveryConfig {
                enabled: true,
                ..Default::default()
            })
    } else {
        // No policy? Use defaults tailored for CLI usage
        assay_core::mcp::policy::DiscoveryConfig {
            enabled: true,
            ..Default::default() // Default methods
        }
    };

    let mut servers = Vec::new();

    if policy_config.enabled {
        // Use methods from policy, or default if empty
        let methods = if policy_config.methods.is_empty() {
            vec![
                assay_core::mcp::policy::DiscoveryMethod::ConfigFiles,
                assay_core::mcp::policy::DiscoveryMethod::Processes,
            ]
        } else {
            policy_config.methods.clone()
        };

        for method in methods {
            match method {
                assay_core::mcp::policy::DiscoveryMethod::ConfigFiles => {
                    let search_paths = get_config_search_paths();
                    servers.extend(scan_config_files(search_paths));
                }
                assay_core::mcp::policy::DiscoveryMethod::Processes => {
                    servers.extend(scan_processes());
                }
                _ => {} // Network/DNS/WellKnown not implemented yet
            }
        }
    }

    let host_info = HostInfo {
        hostname: System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    };

    let summary = InventorySummary {
        total: servers.len(),
        configured: servers
            .iter()
            .filter(|s| s.status == assay_core::discovery::types::ServerStatus::Configured)
            .count(),
        running: servers
            .iter()
            .filter(|s| s.status == assay_core::discovery::types::ServerStatus::Running)
            .count(),
        managed: servers
            .iter()
            .filter(|s| {
                matches!(
                    s.policy_status,
                    assay_core::discovery::types::PolicyStatus::Managed { .. }
                )
            })
            .count(),
        unmanaged: servers
            .iter()
            .filter(|s| {
                matches!(
                    s.policy_status,
                    assay_core::discovery::types::PolicyStatus::Unmanaged
                )
            })
            .count(),
        with_auth: servers
            .iter()
            .filter(|s| {
                s.auth != assay_core::discovery::types::AuthStatus::None
                    && s.auth != assay_core::discovery::types::AuthStatus::Unknown
            })
            .count(),
        without_auth: servers
            .iter()
            .filter(|s| s.auth == assay_core::discovery::types::AuthStatus::None)
            .count(),
    };

    let inventory = Inventory {
        generated_at: chrono::Utc::now(),
        host: host_info,
        servers: servers.clone(),
        summary: summary.clone(),
    };

    match args.format.as_str() {
        "json" => {
            let json_out = serde_json::to_string_pretty(&inventory)?;
            if let Some(out_path) = &args.output {
                std::fs::write(out_path, json_out)?;
            } else {
                println!("{}", json_out);
            }
        }
        "yaml" => {
            let yaml_out = serde_yaml::to_string(&inventory)?;
            if let Some(out_path) = &args.output {
                std::fs::write(out_path, yaml_out)?;
            } else {
                println!("{}", yaml_out);
            }
        }
        _ => {
            // table/text - usually stdout only
            print_table(&inventory);
        }
    }

    use assay_core::mcp::policy::ActionLevel;

    let mut exit_code = 0;

    // Check Unmanaged
    let unmanaged_action = if args
        .fail_on
        .as_ref()
        .map(|v| v.contains(&"unmanaged".to_string()))
        .unwrap_or(false)
    {
        ActionLevel::Fail
    } else {
        policy_config.on_findings.unmanaged_server.clone()
    };

    if summary.unmanaged > 0 {
        match unmanaged_action {
            ActionLevel::Fail => {
                eprintln!("Error: Found {} unmanaged servers.", summary.unmanaged);
                exit_code = 10;
            }
            ActionLevel::Warn => {
                eprintln!("Warning: Found {} unmanaged servers.", summary.unmanaged);
            }
            ActionLevel::Log => {}
        }
    }

    // Check No Auth
    let no_auth_action = if args
        .fail_on
        .as_ref()
        .map(|v| v.contains(&"no_auth".to_string()))
        .unwrap_or(false)
    {
        ActionLevel::Fail
    } else {
        policy_config.on_findings.no_auth.clone()
    };

    if summary.without_auth > 0 {
        match no_auth_action {
            ActionLevel::Fail => {
                eprintln!(
                    "Error: Found {} servers without authentication.",
                    summary.without_auth
                );
                // Keep highest specific error code or just return failure?
                if exit_code == 0 {
                    exit_code = 11;
                }
            }
            ActionLevel::Warn => {
                eprintln!(
                    "Warning: Found {} servers without authentication.",
                    summary.without_auth
                );
            }
            ActionLevel::Log => {}
        }
    }

    Ok(exit_code)
}

fn print_table(inv: &Inventory) {
    println!("🔍 MCP Server Discovery Report");
    println!("   Generated: {}", inv.generated_at);
    println!(
        "   Host: {} ({}/{})",
        inv.host.hostname, inv.host.os, inv.host.arch
    );
    println!();

    if inv.servers.is_empty() {
        println!("No servers found.");
        return;
    }

    // Simple manual table alignment
    println!(
        "{:<30} {:<15} {:<15} {:<15}",
        "SERVER ID", "CLIENT", "STATUS", "AUTH"
    );
    println!(
        "{:<30} {:<15} {:<15} {:<15}",
        "---------", "------", "------", "----"
    );

    for s in &inv.servers {
        let source_client = match &s.source {
            assay_core::discovery::types::DiscoverySource::ConfigFile { client, .. } => {
                client.clone()
            }
            assay_core::discovery::types::DiscoverySource::RunningProcess { .. } => {
                "process".to_string()
            }
            _ => "network".to_string(),
        };

        println!(
            "{:<30} {:<15} {:<15?} {:<15?}",
            s.id.chars().take(29).collect::<String>(),
            source_client,
            s.status,
            s.auth
        );
    }
    println!();
    println!(
        "Summary: Total={}, Unmanaged={}, NoAuth={}",
        inv.summary.total, inv.summary.unmanaged, inv.summary.without_auth
    );
}

fn get_config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // macOS / Linux standard paths
        paths.push(home.join(".config/claude/claude_desktop_config.json"));
        paths.push(home.join("Library/Application Support/Claude/claude_desktop_config.json"));
        paths.push(home.join(".cursor/mcp.json"));
        paths.push(home.join(".vscode/mcp-settings.json"));

        // Windows path
        if cfg!(target_os = "windows") {
            // Prefer %APPDATA% if set (Standard & Testable), else fallback to home/AppData/Roaming
            if let Ok(appdata) = std::env::var("APPDATA") {
                paths.push(PathBuf::from(appdata).join("Claude/claude_desktop_config.json"));
            } else {
                paths.push(home.join("AppData/Roaming/Claude/claude_desktop_config.json"));
            }
        }
    }
    paths
}
