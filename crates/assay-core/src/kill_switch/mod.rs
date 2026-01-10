pub mod killer;
pub mod capture;
pub mod incident;

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum KillMode {
    /// Kill switch semantics: immediate stop (SIGKILL).
    Immediate,
    /// Try SIGTERM first, then SIGKILL after grace period.
    Graceful { grace: Duration },
}

#[derive(Debug, Clone)]
pub struct KillRequest {
    pub pid: u32,
    pub mode: KillMode,
    pub kill_children: bool,
    pub capture_state: bool,
    pub output_dir: Option<PathBuf>, // incident bundle destination
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KillReport {
    pub pid: u32,
    pub success: bool,
    pub children_killed: Vec<u32>,
    pub incident_dir: Option<PathBuf>,
    pub error: Option<String>,
}

pub fn kill_pid(req: KillRequest) -> anyhow::Result<KillReport> {
    killer::kill_pid(req)
}

/// Convenience: parse "proc-12345" OR "12345"
pub fn parse_target_to_pid(s: &str) -> Option<u32> {
    if let Some(rest) = s.strip_prefix("proc-") {
        return rest.parse().ok();
    }
    s.parse().ok()
}
