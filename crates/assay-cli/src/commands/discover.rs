use crate::cli::args::DiscoverArgs;
use assay_core::discovery::{
    config_files::scan_config_files,
    processes::scan_processes,
    types::{DiscoveredServer, Inventory, InventorySummary, HostInfo},
};
use std::collections::HashSet;
use std::path::PathBuf;

pub async fn run(args: DiscoverArgs) -> anyhow::Result<i32> {
    // 1. Gather servers
    let mut servers = Vec::new();

    if args.local {
        // Config files
        let search_paths = get_config_search_paths();
        servers.extend(scan_config_files(search_paths));

        // Processes
        servers.extend(scan_processes());
    }

    // 2. Build Inventory struct
    let host_info = HostInfo {
        hostname: sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    };

    let summary = InventorySummary {
        total: servers.len(),
        configured: servers.iter().filter(|s| s.status == assay_core::discovery::types::ServerStatus::Configured).count(),
        running: servers.iter().filter(|s| s.status == assay_core::discovery::types::ServerStatus::Running).count(),
        managed: servers.iter().filter(|s| match s.policy_status { assay_core::discovery::types::PolicyStatus::Managed{..} => true, _ => false }).count(),
        unmanaged: servers.iter().filter(|s| match s.policy_status { assay_core::discovery::types::PolicyStatus::Unmanaged => true, _ => false }).count(),
        with_auth: servers.iter().filter(|s| s.auth != assay_core::discovery::types::AuthStatus::None && s.auth != assay_core::discovery::types::AuthStatus::Unknown).count(),
        without_auth: servers.iter().filter(|s| s.auth == assay_core::discovery::types::AuthStatus::None).count(),
    };

    let inventory = Inventory {
        generated_at: chrono::Utc::now(),
        host: host_info,
        servers: servers.clone(),
        summary,
    };

    // 3. Output
    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&inventory)?);
        },
        "yaml" => {
            println!("{}", serde_yaml::to_string(&inventory)?);
        },
        _ => { // table/text
            print_table(&inventory);
        }
    }

    // 4. Fail-on check
    if let Some(fail_check) = args.fail_on {
        if fail_check == "unmanaged" && inventory.summary.unmanaged > 0 {
            eprintln!("Error: Found {} unmanaged servers.", inventory.summary.unmanaged);
            return Ok(10); // DISCOVERY_UNMANAGED
        }
        if fail_check == "no_auth" && inventory.summary.without_auth > 0 {
             eprintln!("Error: Found {} servers without authentication.", inventory.summary.without_auth);
            return Ok(11); // DISCOVERY_NO_AUTH
        }
    }

    Ok(0)
}

fn print_table(inv: &Inventory) {
    println!("🔍 MCP Server Discovery Report");
    println!("   Generated: {}", inv.generated_at);
    println!("   Host: {} ({}/{})", inv.host.hostname, inv.host.os, inv.host.arch);
    println!();

    if inv.servers.is_empty() {
        println!("No servers found.");
        return;
    }

    // Simple manual table alignment
    println!("{:<30} {:<15} {:<15} {:<15}", "SERVER ID", "CLIENT", "STATUS", "AUTH");
    println!("{:<30} {:<15} {:<15} {:<15}", "---------", "------", "------", "----");

    for s in &inv.servers {
        let source_client = match &s.source {
            assay_core::discovery::types::DiscoverySource::ConfigFile { client, .. } => client.clone(),
            assay_core::discovery::types::DiscoverySource::RunningProcess { .. } => "process".to_string(),
            _ => "network".to_string(),
        };

        println!("{:<30} {:<15} {:<15?} {:<15?}",
            s.id.chars().take(29).collect::<String>(),
            source_client,
            s.status,
            s.auth
        );
    }
    println!();
    println!("Summary: Total={}, Unmanaged={}, NoAuth={}", inv.summary.total, inv.summary.unmanaged, inv.summary.without_auth);
}

fn get_config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // macOS / Linux standard paths
        paths.push(home.join(".config/claude/claude_desktop_config.json"));
        paths.push(home.join("Library/Application Support/Claude/claude_desktop_config.json"));
        paths.push(home.join(".cursor/mcp.json"));
        paths.push(home.join(".vscode/mcp-settings.json"));
    }
    paths
}
