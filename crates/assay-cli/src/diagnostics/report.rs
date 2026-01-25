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
    pub status: SystemStatus,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LandlockStatus {
    pub available: bool,
    pub fs_enforce: bool,
    pub net_enforce: bool,
    pub abi_version: Option<i32>,
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
