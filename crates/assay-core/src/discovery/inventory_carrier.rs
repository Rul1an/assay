//! `assay.mcp_server_inventory.v0` carrier (MCP09a).
//!
//! A coverage-honest projection of discovered MCP servers into a producer artifact for the OWASP MCP09
//! (shadow-server) line. Discipline, mirroring the E35 experiment that proved the shape:
//!
//! - command/args are HASHED, never emitted raw (they routinely carry secrets and paths);
//! - credential-bearing fields are flagged by key/flag NAME only, never by value;
//! - scanner coverage is declared honestly with an explicit state per source, so an incomplete scan can
//!   never be read as an absence claim.
//!
//! This carrier is the producer half only. Classification against an approved allowlist
//! (shadow/drift/duplicate findings) is a separate consumer concern.

use crate::discovery::types::{DiscoveredServer, DiscoverySource, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The carrier schema id.
pub const SCHEMA: &str = "assay.mcp_server_inventory.v0";

/// The load-bearing non-claim: not observed is not absent unless coverage is complete.
pub const ABSENCE_NON_CLAIM: &str =
    "absence from inventory is not absence from environment unless scanner coverage is complete";

/// Per-source scan coverage. Anything other than `Complete` cannot support an absence claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    NotScanned,
    Unavailable,
    Unsupported,
}

impl CoverageState {
    /// Only a `Complete` scan of a source can support an absence claim for it. Any other state means
    /// "not observed is not absent" - in particular a heuristic scanner must never be `Complete`.
    pub fn supports_absence_claim(self) -> bool {
        matches!(self, CoverageState::Complete)
    }
}

/// What the scan actually covered, declared honestly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerCoverage {
    pub config_sources: BTreeMap<String, CoverageState>,
    pub process_scan: CoverageState,
    pub network_scan: CoverageState,
}

fn digest(value: &Value) -> String {
    let bytes = serde_jcs::to_vec(value).expect("inventory carrier values are JSON-serializable");
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

fn source_id(source: &DiscoverySource) -> String {
    match source {
        DiscoverySource::ConfigFile { client, .. } => format!("{client}_mcp_config"),
        DiscoverySource::RunningProcess { .. } => "process_scan".to_string(),
        DiscoverySource::NetworkScan { .. } => "network_scan".to_string(),
    }
}

/// (transport label, command-or-url, args).
fn transport_parts(transport: &Transport) -> (&'static str, String, Vec<String>) {
    match transport {
        Transport::Stdio { command, args } => ("stdio", command.clone(), args.clone()),
        Transport::Http { url } => ("http", url.clone(), Vec::new()),
        Transport::Sse { url } => ("sse", url.clone(), Vec::new()),
        Transport::WebSocket { url } => ("websocket", url.clone(), Vec::new()),
        Transport::Unknown => ("unknown", String::new(), Vec::new()),
    }
}

fn name_is_credential(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    const NEEDLES: [&str; 8] = [
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "auth",
        "apikey",
        "api_key",
    ];
    NEEDLES.iter().any(|needle| lowered.contains(needle))
        || lowered.split(['_', '-']).any(|part| part == "key")
}

/// A credential-bearing arg flag (by name), e.g. `--token=...` -> `arg:--token`. Never the value.
fn credential_arg_flag(arg: &str) -> Option<String> {
    let flag = arg.split('=').next().unwrap_or(arg);
    let normalized = flag.trim_start_matches('-').to_ascii_lowercase();
    const FLAGS: [&str; 11] = [
        "token",
        "api-key",
        "api_key",
        "apikey",
        "key",
        "secret",
        "password",
        "passwd",
        "auth",
        "authorization",
        "bearer",
    ];
    if FLAGS.contains(&normalized.as_str()) {
        Some(flag.to_string())
    } else {
        None
    }
}

fn credential_indicators(server: &DiscoveredServer, args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for env_key in &server.env_vars {
        if name_is_credential(env_key) {
            out.push(format!("env:{env_key}"));
        }
    }
    for arg in args {
        if let Some(flag) = credential_arg_flag(arg) {
            out.push(format!("arg:{flag}"));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Project discovered servers + scan coverage into the `assay.mcp_server_inventory.v0` carrier.
/// Deterministic: servers are sorted by (server_id, source); only digests and credential names are
/// emitted, never raw command/args or secret values.
pub fn to_inventory_carrier_v0(servers: &[DiscoveredServer], coverage: &ScannerCoverage) -> Value {
    let mut rows: Vec<(String, String, Value)> = servers
        .iter()
        .map(|server| {
            let (transport, mut command, args) = transport_parts(&server.transport);
            // A running-process row carries no transport command, but the source has the cmdline.
            // Hash that (still never raw) so a consumer can identify/compare what was observed rather
            // than seeing every process row collapse to the same empty digest.
            if command.is_empty() {
                if let DiscoverySource::RunningProcess { cmdline, .. } = &server.source {
                    command = cmdline.clone();
                }
            }
            let source = source_id(&server.source);
            let row = json!({
                "server_id": server.id,
                "source": source,
                "transport": transport,
                "command_digest": digest(&json!(command)),
                "args_digest": digest(&json!(args)),
                "credential_indicators": credential_indicators(server, &args),
                "observed_state": "observed",
            });
            (server.id.clone(), source, row)
        })
        .collect();
    rows.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    let servers: Vec<Value> = rows.into_iter().map(|(_, _, row)| row).collect();

    json!({
        "schema": SCHEMA,
        "scanner_coverage": {
            "config_sources": coverage.config_sources,
            "process_scan": coverage.process_scan,
            "network_scan": coverage.network_scan,
        },
        "servers": servers,
        "non_claims": [ABSENCE_NON_CLAIM],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::types::{AuthStatus, PolicyStatus, ServerStatus};
    use std::path::PathBuf;

    fn config_server(
        id: &str,
        client: &str,
        command: &str,
        args: &[&str],
        env: &[&str],
    ) -> DiscoveredServer {
        DiscoveredServer {
            id: id.to_string(),
            name: None,
            source: DiscoverySource::ConfigFile {
                path: PathBuf::from(format!("/cfg/{client}.json")),
                client: client.to_string(),
            },
            transport: Transport::Stdio {
                command: command.to_string(),
                args: args.iter().map(|a| a.to_string()).collect(),
            },
            status: ServerStatus::Configured,
            policy_status: PolicyStatus::Unmanaged,
            auth: AuthStatus::Unknown,
            env_vars: env.iter().map(|e| e.to_string()).collect(),
            risk_hints: Vec::new(),
        }
    }

    fn fixture_coverage() -> ScannerCoverage {
        let mut config_sources = BTreeMap::new();
        config_sources.insert("claude_desktop".to_string(), CoverageState::Complete);
        config_sources.insert("cursor".to_string(), CoverageState::NotScanned);
        ScannerCoverage {
            config_sources,
            process_scan: CoverageState::Unavailable,
            network_scan: CoverageState::Unsupported,
        }
    }

    fn golden_fixture_servers() -> Vec<DiscoveredServer> {
        vec![
            config_server(
                "github-tools",
                "claude_desktop",
                "npx",
                &["-y", "@modelcontextprotocol/server-github"],
                &["GITHUB_TOKEN"],
            ),
            config_server("notes", "claude_desktop", "node", &["/opt/notes.js"], &[]),
        ]
    }

    #[test]
    fn carrier_matches_golden_fixture() {
        let carrier = to_inventory_carrier_v0(&golden_fixture_servers(), &fixture_coverage());
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp_server_inventory_v0.golden.json");
        if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&carrier).unwrap()),
            )
            .unwrap();
        }
        let committed: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
                    path.display()
                )
            }))
            .unwrap();
        assert_eq!(
            committed, carrier,
            "mcp_server_inventory.v0 golden is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
        );
    }

    #[test]
    fn carrier_shape_is_pinned() {
        let servers = vec![
            config_server(
                "github-tools",
                "claude_desktop",
                "npx",
                &["-y", "@modelcontextprotocol/server-github"],
                &["GITHUB_TOKEN"],
            ),
            config_server("notes", "claude_desktop", "node", &["/opt/notes.js"], &[]),
        ];
        let carrier = to_inventory_carrier_v0(&servers, &fixture_coverage());

        assert_eq!(carrier["schema"], SCHEMA);
        assert_eq!(carrier["non_claims"][0], ABSENCE_NON_CLAIM);
        assert_eq!(carrier["scanner_coverage"]["process_scan"], "unavailable");
        assert_eq!(carrier["scanner_coverage"]["network_scan"], "unsupported");
        assert_eq!(
            carrier["scanner_coverage"]["config_sources"]["claude_desktop"],
            "complete"
        );

        let rows = carrier["servers"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        // sorted by server_id: github-tools before notes
        assert_eq!(rows[0]["server_id"], "github-tools");
        assert_eq!(rows[1]["server_id"], "notes");
        // credential flagged by NAME only
        assert_eq!(rows[0]["credential_indicators"][0], "env:GITHUB_TOKEN");
        assert!(rows[1]["credential_indicators"]
            .as_array()
            .unwrap()
            .is_empty());
        // command/args are digests, not raw
        assert!(rows[0]["command_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        assert!(rows[0]["args_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
    }

    #[test]
    fn no_raw_command_or_args_or_secret_values_leak() {
        let servers = vec![config_server(
            "x",
            "claude_desktop",
            "npx",
            &["-y", "@modelcontextprotocol/server-github"],
            &["GITHUB_TOKEN"],
        )];
        let carrier = to_inventory_carrier_v0(&servers, &fixture_coverage());
        let blob = serde_json::to_string(&carrier).unwrap();
        // The row carries digests + the credential field NAME, never the raw command/args values.
        for forbidden in ["npx", "@modelcontextprotocol/server-github"] {
            assert!(
                !blob.contains(forbidden),
                "carrier must not leak `{forbidden}`"
            );
        }
        // ...and never the raw command/args KEYS (only command_digest / args_digest).
        let row = &carrier["servers"][0];
        assert!(row.get("command").is_none() && row.get("args").is_none());
    }

    #[test]
    fn http_endpoint_hashes_url_and_has_empty_args() {
        let server = DiscoveredServer {
            id: "remote".to_string(),
            name: None,
            source: DiscoverySource::ConfigFile {
                path: PathBuf::from("/cfg/cursor.json"),
                client: "cursor".to_string(),
            },
            transport: Transport::Http {
                url: "http://localhost:8080/mcp".to_string(),
            },
            status: ServerStatus::Configured,
            policy_status: PolicyStatus::Unmanaged,
            auth: AuthStatus::Unknown,
            env_vars: Vec::new(),
            risk_hints: Vec::new(),
        };
        let carrier = to_inventory_carrier_v0(&[server], &fixture_coverage());
        let row = &carrier["servers"][0];
        assert_eq!(row["transport"], "http");
        assert_eq!(row["source"], "cursor_mcp_config");
        assert!(!serde_json::to_string(&carrier)
            .unwrap()
            .contains("localhost:8080"));
    }

    fn process_server(id: &str, pid: u32, cmdline: &str) -> DiscoveredServer {
        DiscoveredServer {
            id: id.to_string(),
            name: None,
            source: DiscoverySource::RunningProcess {
                pid,
                cmdline: cmdline.to_string(),
                started_at: None,
                user: None,
            },
            transport: Transport::Unknown,
            status: ServerStatus::Running,
            policy_status: PolicyStatus::Unmanaged,
            auth: AuthStatus::Unknown,
            env_vars: Vec::new(),
            risk_hints: Vec::new(),
        }
    }

    #[test]
    fn process_rows_hash_distinct_cmdlines_without_leaking_them() {
        let servers = vec![
            process_server("proc-1", 1, "node /opt/a/mcp-server.js --port 1"),
            process_server("proc-2", 2, "node /opt/b/mcp-server.js --port 2"),
        ];
        let carrier = to_inventory_carrier_v0(&servers, &fixture_coverage());
        let rows = carrier["servers"].as_array().unwrap();
        // Distinct cmdlines -> distinct command digests (not the empty-input collision).
        assert_ne!(rows[0]["command_digest"], rows[1]["command_digest"]);
        assert!(rows[0]["command_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        // ...and the raw cmdline never appears in the carrier.
        let blob = serde_json::to_string(&carrier).unwrap();
        assert!(!blob.contains("/opt/a/mcp-server.js") && !blob.contains("--port"));
    }

    #[test]
    fn only_complete_coverage_supports_an_absence_claim() {
        assert!(CoverageState::Complete.supports_absence_claim());
        for state in [
            CoverageState::Partial,
            CoverageState::NotScanned,
            CoverageState::Unavailable,
            CoverageState::Unsupported,
        ] {
            assert!(
                !state.supports_absence_claim(),
                "{state:?} must not support an absence claim"
            );
        }
    }
}
