//! `assay mcp inventory` (MCP09a): emit the `assay.mcp_server_inventory.v0` carrier.
//!
//! Reuses the existing discovery (config files + processes), then projects it into the coverage-honest
//! carrier (hashed command/args, credential fields by name, explicit per-source coverage). This is the
//! producer half; classification against an approved allowlist is a separate consumer concern.

use crate::cli::args::InventoryArgs;
use assay_core::discovery::config_files::scan_config_files;
use assay_core::discovery::inventory_carrier::{
    to_inventory_carrier_v0, CoverageState, ScannerCoverage,
};
use assay_core::discovery::processes::scan_processes;
use assay_core::discovery::types::DiscoverySource;
use std::collections::BTreeMap;
use std::io::Write;

use super::config_path::{detect_config_path, McpClient};

pub async fn run(args: InventoryArgs) -> anyhow::Result<i32> {
    let mut servers = scan_config_files(super::discover::get_config_search_paths());
    // --no-process-scan scopes the inventory to config sources only. We then report process_scan as
    // not_scanned (we did not look), never partial: honest about coverage and deterministic (no
    // host-dependent process rows), which is what reproducible inventory review needs.
    if !args.no_process_scan {
        servers.extend(scan_processes());
    }

    // Coverage is declared honestly per source. A config client whose path exists was scanned
    // (complete); a resolvable-but-absent path is not_scanned (we cannot claim it is absent); an
    // unresolvable client is unsupported. A client that actually yielded a server was scanned.
    let mut config_sources: BTreeMap<String, CoverageState> = BTreeMap::new();
    for (client, key) in [
        (McpClient::Claude, "claude_desktop"),
        (McpClient::Cursor, "cursor"),
    ] {
        let state = match detect_config_path(client) {
            Some(path) if path.exists() => CoverageState::Complete,
            Some(_) => CoverageState::NotScanned,
            None => CoverageState::Unsupported,
        };
        config_sources.insert(key.to_string(), state);
    }
    for server in &servers {
        if let DiscoverySource::ConfigFile { client, .. } = &server.source {
            config_sources.insert(client.clone(), CoverageState::Complete);
        }
    }

    let coverage = ScannerCoverage {
        config_sources,
        process_scan: process_scan_coverage(args.no_process_scan),
        network_scan: CoverageState::Unsupported,
    };

    let carrier = to_inventory_carrier_v0(&servers, &coverage);
    let rendered = format!("{}\n", serde_json::to_string_pretty(&carrier)?);

    if args.out == "-" {
        std::io::stdout().write_all(rendered.as_bytes())?;
    } else {
        std::fs::write(&args.out, rendered)?;
    }
    Ok(crate::exit_codes::EXIT_SUCCESS)
}

/// Process-scan coverage. With `--no-process-scan` the scan is scoped out, so coverage is
/// `NotScanned` (we did not look) - honest and absence-unsupporting. Otherwise process discovery is a
/// cmdline substring heuristic (`mcp-server` / `@modelcontextprotocol/server` / `mcp_server`), not an
/// exhaustive enumeration, so it is `Partial`: it can surface a running server but can never support an
/// absence claim. Neither state ever claims absence; only a `Complete` scan could.
fn process_scan_coverage(no_process_scan: bool) -> CoverageState {
    if no_process_scan {
        CoverageState::NotScanned
    } else {
        CoverageState::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_process_coverage_never_supports_absence() {
        assert_eq!(process_scan_coverage(false), CoverageState::Partial);
        assert!(!process_scan_coverage(false).supports_absence_claim());
    }

    #[test]
    fn scoped_out_process_coverage_is_not_scanned_and_unsupporting() {
        assert_eq!(process_scan_coverage(true), CoverageState::NotScanned);
        assert!(!process_scan_coverage(true).supports_absence_claim());
    }
}
