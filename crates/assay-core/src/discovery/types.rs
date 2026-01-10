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

impl Inventory {
    /// All PIDs of servers that were discovered as running processes.
    pub fn running_pids(&self) -> Vec<u32> {
        let mut pids = Vec::new();

        for s in &self.servers {
            if s.status != ServerStatus::Running {
                continue;
            }
            if let Some(pid) = s.pid_if_running_process() {
                pids.push(pid);
            }
        }

        pids.sort_unstable();
        pids.dedup();
        pids
    }

    /// Resolve an input token to a PID.
    pub fn resolve_to_pid(&self, token: &str) -> Option<u32> {
        // 1) Direct PID forms (inline parsing logic to avoid hard dep on kill_switch)
        let pid_opt = if let Some(rest) = token.strip_prefix("proc-") {
            rest.parse::<u32>().ok()
        } else {
            token.parse::<u32>().ok()
        };

        if let Some(pid) = pid_opt {
            return Some(pid);
        }

        // 2) Exact match by server id
        if let Some(s) = self.servers.iter().find(|s| s.id == token) {
            return s.pid_if_running_process();
        }

        // 3) Match by name (prefer Running status)
        let mut candidates = self
            .servers
            .iter()
            .filter(|s| s.name.as_deref() == Some(token));

        // Prefer running process entries
        if let Some(s) = candidates
            .clone()
            .find(|s| s.status == ServerStatus::Running && s.pid_if_running_process().is_some())
        {
            return s.pid_if_running_process();
        }

        // Fallback: any running-process entry
        if let Some(s) = candidates.find(|s| s.pid_if_running_process().is_some()) {
            return s.pid_if_running_process();
        }

        None
    }

    /// Convenience for `assay kill --all`
    pub fn running_process_servers(&self) -> Vec<RunningProcessServer> {
        let mut out: Vec<RunningProcessServer> = self
            .servers
            .iter()
            .filter(|s| s.status == ServerStatus::Running)
            .filter_map(|s| match &s.source {
                DiscoverySource::RunningProcess { pid, cmdline, .. } => Some(RunningProcessServer {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    pid: *pid,
                    cmdline: cmdline.clone(),
                }),
                _ => None,
            })
            .collect();

        out.sort_by_key(|x| x.pid);
        out.dedup_by_key(|x| x.pid);
        out
    }
}

impl DiscoveredServer {
    pub fn pid_if_running_process(&self) -> Option<u32> {
        match self.source {
            DiscoverySource::RunningProcess { pid, .. } => Some(pid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunningProcessServer {
    pub id: String,
    pub name: Option<String>,
    pub pid: u32,
    pub cmdline: String,
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
