use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize, Clone)]
pub struct DiagnosticReport {
    pub assay_version: String,
    pub platform: String,
    pub kernel: Option<String>,
    pub lsms: Vec<String>,
    pub landlock: LandlockStatus,
    pub bpf_lsm: BpfLsmStatus,
    pub helper: HelperStatus,
    pub backend: BackendSelection,
    pub sandbox_features: SandboxFeatures,
    pub status: SystemStatus,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LandlockStatus {
    pub available: bool,
    pub fs_enforce: bool,
    pub net_enforce: bool,
    pub abi_version: Option<u32>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BpfLsmStatus {
    pub available: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct HelperStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub version: Option<String>,
    pub socket: PathBuf,
    pub socket_exists: bool,
    // Future: caps status
}

#[derive(Debug, Serialize, Clone)]
pub struct BackendSelection {
    pub selected: String,
    pub mode: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum SystemStatus {
    Ready,
    Degraded,
    Unsupported,
}

/// Phase 5 sandbox hardening features status
#[derive(Debug, Serialize, Clone)]
pub struct SandboxFeatures {
    /// Environment variable scrubbing enabled
    pub env_scrubbing: bool,
    /// Scoped /tmp with proper permissions
    pub scoped_tmp: bool,
    /// Fork-safe pre_exec (no allocations)
    pub fork_safe_preexec: bool,
    /// Deny-wins conflict detection
    pub deny_conflict_detection: bool,
}

impl Default for SandboxFeatures {
    fn default() -> Self {
        Self {
            env_scrubbing: true,
            scoped_tmp: true,
            fork_safe_preexec: true,
            deny_conflict_detection: true,
        }
    }
}
