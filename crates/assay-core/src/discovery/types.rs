use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub host: HostInfo,
    pub servers: Vec<DiscoveredServer>,
    pub summary: InventorySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub hostname: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySummary {
    pub total: usize,
    pub configured: usize,
    pub running: usize,
    pub managed: usize,
    pub unmanaged: usize,
    pub with_auth: usize,
    pub without_auth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServer {
    pub id: String,
    pub name: Option<String>,
    pub source: DiscoverySource,
    pub transport: Transport,
    pub status: ServerStatus,
    pub policy_status: PolicyStatus,
    pub auth: AuthStatus,
    #[serde(default)]
    pub env_vars: Vec<String>, // Names only, strictly no values
    #[serde(default)]
    pub risk_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiscoverySource {
    ConfigFile {
        path: PathBuf,
        client: String, // "claude_desktop", "cursor", "vscode", "generic"
    },
    RunningProcess {
        pid: u32,
        cmdline: String,
        started_at: Option<String>,
        user: Option<String>,
    },
    NetworkScan {
        address: String,
        port: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transport {
    Stdio {
        command: String,
        args: Vec<String>,
    },
    Http {
        url: String,
    },
    Sse {
        url: String,
    },
    WebSocket {
        url: String,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Configured, // Found in config, not correlated to running process
    Running,    // Active process
    Listening,  // Network endpoint responding
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus {
    Managed { policy_file: PathBuf },
    Unmanaged,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    None,
    ApiKey, // Inferred from env vars like *_KEY, *_TOKEN
    OAuth,
    Mtls,
    Unknown,
}
