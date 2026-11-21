use serde::Serialize;
use std::path::PathBuf;

assay_common::closed! {
    #[derive(Serialize)]
    root pub struct Inventory {
        generated_at: chrono::DateTime<chrono::Utc>,
        host: HostInfo,
        servers: Vec<DiscoveredServer>,
        summary: InventorySummary,
    }
}

assay_common::closed! {
    #[derive(Serialize)]
    pub struct HostInfo {
        hostname: String,
        os: String,
        arch: String,
    }
}

assay_common::closed! {
    #[derive(Serialize)]
    pub struct InventorySummary {
        total: usize,
        configured: usize,
        running: usize,
        managed: usize,
        unmanaged: usize,
        with_auth: usize,
        without_auth: usize,
    }
}

assay_common::closed! {
    #[derive(Serialize)]
    pub struct DiscoveredServer {
        id: String,
        name: Option<String>,
        source: DiscoverySource,
        transport: Transport,
        status: ServerStatus,
        policy_status: PolicyStatus,
        auth: AuthStatus,
        #[default]
        env_vars: Vec<String>, // Names only, strictly no values
        #[default]
        risk_hints: Vec<String>,
    }
}

assay_common::closed! {
    legacy_serde pub enum DiscoverySource tagged "type" {
        "config_file" => ConfigFile {
            path: PathBuf,
            client: String, // "claude_desktop", "cursor", "vscode", "generic"
        },
        "running_process" => RunningProcess {
            pid: u32,
            cmdline: String,
            started_at: Option<String>,
            user: Option<String>,
        },
        "network_scan" => NetworkScan {
            address: String,
            port: u16,
        },
    }
}

assay_common::closed! {
    legacy_serde pub enum Transport tagged "type" {
        "stdio" => Stdio { command: String, args: Vec<String> },
        "http" => Http { url: String },
        "sse" => Sse { url: String },
        "web_socket" => WebSocket { url: String },
        "unknown" => Unknown,
    }
}

assay_common::closed! {
    #[derive(Serialize)]
    pub enum ServerStatus {
        "configured" => Configured, // Found in config, not correlated to running process
        "running" => Running,    // Active process
        "listening" => Listening,  // Network endpoint responding
        "unreachable" => Unreachable,
        "unknown" => Unknown,
    }
}

assay_common::closed! {
    pub enum PolicyStatus external {
        "managed" => Managed { policy_file: PathBuf },
        "unmanaged" => Unmanaged,
        "unknown" => Unknown,
    }
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

    /// Convenience for `assay mcp kill --all`
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only a running process has a pid and cmdline to project; other sources carry neither. On the fn rather than the match: the match sits inside a `.filter_map` closure mid-chain, where an attribute is not a valid position"
    )]
    pub fn running_process_servers(&self) -> Vec<RunningProcessServer> {
        let mut out: Vec<RunningProcessServer> = self
            .servers
            .iter()
            .filter(|s| s.status == ServerStatus::Running)
            .filter_map(|s| match &s.source {
                DiscoverySource::RunningProcess { pid, cmdline, .. } => {
                    Some(RunningProcessServer {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        pid: *pid,
                        cmdline: cmdline.clone(),
                    })
                }
                _ => None,
            })
            .collect();

        out.sort_by_key(|x| x.pid);
        out.dedup_by_key(|x| x.pid);
        out
    }
}

impl DiscoveredServer {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only a running process has a pid"
    )]
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

assay_common::closed! {
    #[derive(Serialize)]
    pub enum AuthStatus {
        "none" => None,
        "api_key" => ApiKey, // Inferred from env vars like *_KEY, *_TOKEN
        "o_auth" => OAuth,
        "mtls" => Mtls,
        "unknown" => Unknown,
    }
}
