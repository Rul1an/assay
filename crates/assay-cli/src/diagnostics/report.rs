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
    pub metrics: std::collections::HashMap<String, u64>,
    pub status: SystemStatus,
    pub suggestions: Vec<String>,
}

/// How the Landlock ABI probe resolved. The authoritative query is the
/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` syscall (per the kernel docs),
/// NOT the `/sys/kernel/security/landlock/abi_version` path, which does not exist on mainline kernels
/// and produced a false-negative `net_enforce` on real hosts.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LandlockAbiProbeStatus {
    /// Syscall returned an ABI version (>= 1): Landlock is built in and enabled.
    Ok,
    /// Syscall returned `ENOSYS`: the kernel has no Landlock support.
    Unsupported,
    /// Syscall returned `EOPNOTSUPP`: Landlock is built in but disabled at boot.
    Disabled,
    /// Any other errno (or a non-Linux platform): could not be determined.
    Error,
}

/// How the Landlock-net CONNECT_TCP ruleset usability smoke resolved. One step beyond
/// `LandlockAbiProbeStatus`: it does not just read the ABI, it actually builds a TCP-connect
/// ruleset and applies it (`landlock_restrict_self`) in a throwaway child. This is
/// host-eligibility/diagnostics only and does NOT implement or claim enforcement.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LandlockNetProbeStatus {
    /// Ruleset created, a port rule added, `no_new_privs` set, and `restrict_self` succeeded
    /// in the child: the host supports the CONNECT_TCP syscall path needed for future enforcement.
    Usable,
    /// ABI < 4 (no `LANDLOCK_ACCESS_NET_CONNECT_TCP`) or non-Linux: the smoke is not applicable.
    Unsupported,
    /// Ruleset build, `no_new_privs`, or `restrict_self` failed (errno carried separately).
    Failed,
}

#[derive(Debug, Serialize, Clone)]
pub struct LandlockStatus {
    /// Landlock present in `/sys/kernel/security/lsm`. Unchanged meaning; kept as an extra observation
    /// only — it is NOT the source of truth for ABI or net support (the syscall is).
    pub available: bool,
    pub fs_enforce: bool,
    /// Back-compat alias for `net_connect_tcp_supported` (kept so existing readers do not break).
    pub net_enforce: bool,
    pub abi_version: Option<u32>,
    /// How `abi_version` was obtained: `landlock_create_ruleset_version` (syscall) or `none`.
    pub abi_version_source: &'static str,
    pub abi_probe_status: LandlockAbiProbeStatus,
    /// The errno from the ABI probe when it did not return a version; `None` on success.
    pub abi_probe_errno: Option<i32>,
    /// ABI >= 4 (`LANDLOCK_ACCESS_NET_CONNECT_TCP`). Required for the future Landlock TCP-connect path.
    pub net_connect_tcp_supported: bool,
    /// ABI >= 4 (`LANDLOCK_ACCESS_NET_BIND_TCP`).
    pub net_bind_tcp_supported: bool,
    /// Whether `PR_SET_NO_NEW_PRIVS` could be set in a throwaway child (prerequisite for unprivileged
    /// `landlock_restrict_self`). Measured in a forked child, never set on the diagnostics process.
    pub no_new_privs_settable: bool,
    /// Result of the CONNECT_TCP ruleset usability smoke: whether the host can create and apply a
    /// TCP-connect Landlock ruleset (`restrict_self`) in a throwaway child. Host-eligibility only;
    /// proves nothing about whether any connection is actually blocked.
    pub net_connect_ruleset_probe: LandlockNetProbeStatus,
    /// The errno when `net_connect_ruleset_probe` is `failed`; `None` on `usable`/`unsupported`.
    pub net_connect_ruleset_errno: Option<i32>,
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
