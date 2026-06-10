//! Sandbox backend abstraction and implementations.
//! Implements ADR-001 Backend Strategy: BPF -> Landlock -> NoopAudit fallback.
//!
//! Security model:
//! - Landlock is ALLOW-only (no explicit deny rules)
//! - Detection is probe-based, not kernel version guessing
//! - Enforcement happens in child process via pre_exec

/// Capabilities a backend can provide.
#[derive(Debug, Clone, Default)]
pub struct BackendCaps {
    pub enforce_fs: bool,
    pub enforce_net: bool,
    pub enforce_ioctl: bool,
    pub enforce_scopes: bool,
    pub audit_only: bool,
    /// Detected ABI version (for Landlock)
    pub abi_version: u32,
}

/// Backend selection result.
#[derive(Debug, Clone)]
pub enum BackendType {
    Landlock,
    NoopAudit, // Fallback: no enforcement, just run and trace
    #[allow(dead_code)]
    Bpf, // Future: when helper is available
}

impl BackendType {
    pub fn name(&self) -> &'static str {
        match self {
            BackendType::Landlock => "Landlock",
            BackendType::NoopAudit => "NoopAudit",
            BackendType::Bpf => "BPF-LSM",
        }
    }
}

/// Detect available backend based on actual system probing.
pub fn detect_backend() -> (BackendType, BackendCaps) {
    let (supported, abi) = landlock_impl::probe_abi();
    if supported {
        (
            BackendType::Landlock,
            BackendCaps {
                enforce_fs: abi >= 1,
                enforce_net: abi >= 4,
                enforce_ioctl: abi >= 5,
                enforce_scopes: abi >= 6,
                audit_only: false,
                abi_version: abi,
            },
        )
    } else {
        // Fallback: no enforcement (macOS, old Linux, etc.)
        (
            BackendType::NoopAudit,
            BackendCaps {
                enforce_fs: false,
                enforce_net: false,
                enforce_ioctl: false,
                enforce_scopes: false,
                audit_only: true,
                abi_version: 0,
            },
        )
    }
}

/// Handle for enforcing Landlock restrictions.
pub struct LandlockEnforcer {
    #[cfg(target_os = "linux")]
    ruleset: Option<landlock_impl::RulesetHandle>,
}

impl LandlockEnforcer {
    /// Enforce the ruleset. Safe to call from pre_exec.
    ///
    /// # Safety Note
    /// This MUST be called from pre_exec (after fork, before exec).
    /// The implementation ONLY calls syscalls (prctl + landlock_restrict_self).
    /// No heap allocations occur in the success path.
    /// On failure, std::io::Error is constructed but this is a rare error case
    /// where the child will exit anyway.
    pub fn enforce(&mut self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(ruleset) = self.ruleset.take() {
            return landlock_impl::enforce_fork_safe(ruleset);
        }
        Ok(())
    }
}

/// Prepare Landlock ruleset. allocations/IO allowed here. `net_allow_ports` of `Some` builds a
/// combined FS+NET (TCP-connect allowlist) ruleset; `None` is FS-only (unchanged behaviour).
pub fn prepare_landlock(
    policy: &crate::policy::Policy,
    scoped_tmp: &std::path::Path,
    net_allow_ports: Option<&[u16]>,
) -> anyhow::Result<LandlockEnforcer> {
    #[cfg(target_os = "linux")]
    {
        let ruleset = landlock_impl::create_ruleset(policy, scoped_tmp, net_allow_ports)?;
        Ok(LandlockEnforcer {
            ruleset: Some(ruleset),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = policy;
        let _ = scoped_tmp;
        let _ = net_allow_ports;
        Ok(LandlockEnforcer {})
    }
}

// =============================================================================
// Linux-only Landlock implementation
// =============================================================================
#[cfg(target_os = "linux")]
mod landlock_impl {
    use landlock::{
        Access, AccessFs, AccessNet, BitFlags, CompatLevel, Compatible, NetPort, PathBeneath,
        PathFd, Ruleset, RulesetAttr, RulesetCreated, RulesetCreatedAttr, RulesetStatus, ABI,
    };
    use std::path::Path;

    /// Explicit system files that are safe to READ ONLY.
    const SYSTEM_READ_FILES: &[&str] = &[
        "/etc/hosts",
        "/etc/resolv.conf",
        "/etc/localtime",
        "/etc/timezone",
        "/etc/ld.so.cache",
    ];

    /// System directories that require READ + EXECUTE (for binaries/libs).
    const SYSTEM_RUNTIME_DIRS: &[&str] = &["/usr", "/lib", "/lib64", "/bin", "/sbin"];

    /// Probe-based Landlock ABI detection.
    pub(super) fn probe_abi() -> (bool, u32) {
        let mut max_abi;
        // Check V1 (FS Read/Write/Exec)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 1;
        } else {
            return (false, 0);
        }

        // Check V2 (FS Refer)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V2))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 2;
        } else {
            return (true, max_abi);
        }

        // Check V3 (FS Truncate)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V3))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 3;
        } else {
            return (true, max_abi);
        }

        // Check V4 (Net TCP)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V4))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 4;
        }

        // TODO(landlock-abi-v5): ABI v5/v6/v7 when landlock crate or raw syscalls ready

        (true, max_abi)
    }

    pub(super) type RulesetHandle = RulesetCreated;

    /// Fork-safe enforcement: only syscalls, returns errno on failure.
    pub(super) fn enforce_fork_safe(ruleset: RulesetHandle) -> std::io::Result<()> {
        match ruleset.restrict_self() {
            Ok(status) => {
                if status.ruleset == RulesetStatus::NotEnforced {
                    return Err(std::io::Error::from_raw_os_error(libc::ENOTSUP));
                }
                Ok(())
            }
            Err(_e) => Err(std::io::Error::from_raw_os_error(libc::EPERM)),
        }
    }

    /// Create Landlock ruleset from policy. When `net_allow_ports` is `Some`, a combined FS+NET
    /// ruleset is built in ONE ruleset: `LANDLOCK_ACCESS_NET_CONNECT_TCP` is handled as a hard
    /// requirement (no best-effort downgrade for an enforcement claim) and a `NetPort` allow rule is
    /// added for each port. An empty port list is a valid deny-all-TCP-connect. When `None`, the
    /// ruleset is FS-only and unchanged.
    pub(super) fn create_ruleset(
        policy: &crate::policy::Policy,
        scoped_tmp: &Path,
        net_allow_ports: Option<&[u16]>,
    ) -> anyhow::Result<RulesetCreated> {
        let (_, abi_level) = probe_abi();
        let abi = match abi_level {
            1 => ABI::V1,
            2 => ABI::V2,
            3 => ABI::V3,
            _ => ABI::V4, // Default to V4 for probing higher
        };

        let mut ruleset = Ruleset::default();

        // FS rules (ABI V1-V3)
        ruleset = ruleset.handle_access(AccessFs::from_all(abi))?;

        // NET rules (ABI V4): handle CONNECT_TCP as a hard requirement so an unsupported host fails
        // rather than silently enforcing nothing. Allowlist-only: once handled, TCP connects are
        // denied unless a NetPort rule below grants the destination port.
        if net_allow_ports.is_some() {
            ruleset = ruleset
                .set_compatibility(CompatLevel::HardRequirement)
                .handle_access(AccessNet::ConnectTcp)?
                .set_compatibility(CompatLevel::BestEffort);
        }

        let mut ruleset = ruleset.create()?;

        if let Some(ports) = net_allow_ports {
            for &port in ports {
                ruleset = ruleset.add_rule(NetPort::new(port, AccessNet::ConnectTcp))?;
            }
        }

        // 1. Allow CWD (RX only by default, safe containment)
        if let Ok(cwd) = std::env::current_dir() {
            ruleset = add_path(
                ruleset,
                cwd.to_string_lossy().as_ref(),
                AccessFs::from_read(abi) | AccessFs::Execute,
            )?;
        }

        // 2. Allow scoped /tmp (RWX)
        ruleset = add_path(
            ruleset,
            scoped_tmp.to_string_lossy().as_ref(),
            AccessFs::from_all(abi),
        )?;

        // 3. Allow system files (Read Only)
        for path in SYSTEM_READ_FILES {
            ruleset = add_path(ruleset, path, AccessFs::from_read(abi))?;
        }

        // 4. Allow system runtime dirs (Read + Execute)
        let rx = AccessFs::from_read(abi) | AccessFs::Execute;
        for path in SYSTEM_RUNTIME_DIRS {
            ruleset = add_path(ruleset, path, rx)?;
        }

        // 5. Allow paths from policy.fs.allow
        for path in &policy.fs.allow {
            let expanded = expand_path(path);
            ruleset = add_path(ruleset, &expanded, AccessFs::from_all(abi))?;
        }

        Ok(ruleset)
    }

    /// Helper to safely add rules.
    /// Takes ownership of ruleset and returns it to satisfy daisy-chaining requirements.
    fn add_path(
        ruleset: RulesetCreated,
        path: &str,
        access: BitFlags<AccessFs>,
    ) -> anyhow::Result<RulesetCreated> {
        if Path::new(path).exists() {
            match PathFd::new(path) {
                Ok(fd) => {
                    return Ok(ruleset.add_rule(PathBeneath::new(fd, access))?);
                }
                Err(e) => {
                    // Log but don't fail hard if a single connection fails (best effort)
                    eprintln!("WARN: Landlock failed to open path '{}': {}", path, e);
                }
            }
        }
        Ok(ruleset)
    }

    /// Expand ~ and ${HOME} in paths.
    fn expand_path(path: &str) -> String {
        let mut expanded = path.to_string();

        if expanded.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                expanded = expanded.replacen("~", &home, 1);
            }
        }

        // Simple variable expansion
        if expanded.contains("${HOME}") {
            if let Ok(home) = std::env::var("HOME") {
                expanded = expanded.replace("${HOME}", &home);
            }
        }
        if expanded.contains("${CWD}") {
            if let Ok(cwd) = std::env::current_dir() {
                expanded = expanded.replace("${CWD}", &cwd.to_string_lossy());
            }
        }
        if expanded.contains("${USER}") {
            if let Ok(user) = std::env::var("USER") {
                expanded = expanded.replace("${USER}", &user);
            }
        }

        expanded
    }

    /// Build a NET-only Landlock ruleset that handles `CONNECT_TCP` (hard requirement) and allows the
    /// given TCP-connect ports. Used by the self-probe: connecting to a port NOT in this list under
    /// the applied ruleset must be denied. FS is intentionally not handled here (the probe only tests
    /// the network rule; Landlock FS does not affect sockets).
    pub(super) fn build_net_ruleset(allowed_ports: &[u16]) -> anyhow::Result<RulesetCreated> {
        let mut ruleset = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessNet::ConnectTcp)?
            .create()?;
        for &port in allowed_ports {
            ruleset = ruleset.add_rule(NetPort::new(port, AccessNet::ConnectTcp))?;
        }
        Ok(ruleset)
    }
}

/// Outcome of the enforcement self-probe: the result of a single connect to a denied port, attempted
/// from inside a child that applied the Landlock-net ruleset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfProbeOutcome {
    /// The denied connect failed with `EACCES`: the block held (the proven-block signal).
    BlockedEacces,
    /// The connect SUCCEEDED: the ruleset did NOT block it (block failed).
    Connected,
    /// The connect failed with some other errno (a weak signal, never a proven block).
    OtherErrno(i32),
    /// The probe could not run (no_new_privs / restrict_self / socket failed). Must be surfaced, not
    /// silently treated as a block.
    ProbeInfraError(&'static str),
}

/// Run the enforcement self-probe: in a throwaway child, set `no_new_privs`, apply a NET ruleset that
/// allows `allowed_ports`, then attempt a TCP connect to `127.0.0.1:deny_port` (which is NOT allowed).
/// The child reports the outcome via its exit code; the parent never assumes a block. AS-safe: only
/// raw syscalls run between fork and `_exit`.
#[cfg(target_os = "linux")]
pub fn self_probe_denied_connect(
    allowed_ports: &[u16],
    deny_port: u16,
) -> anyhow::Result<SelfProbeOutcome> {
    use std::os::fd::AsRawFd;

    // Sentinels well above the connect-errno range so an infra failure is never read as an errno.
    const EXIT_NNP_FAILED: i32 = 200;
    const EXIT_RESTRICT_FAILED: i32 = 201;
    const EXIT_SOCKET_FAILED: i32 = 202;
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;

    let created = landlock_impl::build_net_ruleset(allowed_ports)?;
    let owned: Option<std::os::fd::OwnedFd> = created.into();
    let raw_fd = owned.as_ref().map(|f| f.as_raw_fd()).unwrap_or(-1);

    // SAFETY: the child runs only async-signal-safe syscalls (prctl, landlock_restrict_self, socket,
    // connect, _exit). No allocations, no Rust destructors (we _exit). The parent only waitpid.
    let outcome = unsafe {
        let pid = libc::fork();
        if pid == 0 {
            if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                libc::_exit(EXIT_NNP_FAILED);
            }
            if libc::syscall(libc::SYS_landlock_restrict_self, raw_fd, 0) != 0 {
                libc::_exit(EXIT_RESTRICT_FAILED);
            }
            let sfd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            if sfd < 0 {
                libc::_exit(EXIT_SOCKET_FAILED);
            }
            let addr = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: deny_port.to_be(),
                sin_addr: libc::in_addr {
                    s_addr: 0x7f00_0001u32.to_be(), // 127.0.0.1
                },
                sin_zero: [0; 8],
            };
            let ret = libc::connect(
                sfd,
                std::ptr::addr_of!(addr) as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            );
            if ret == 0 {
                libc::_exit(0); // connected => block FAILED
            }
            let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            let code = if e <= 0 || e > 199 { 199 } else { e };
            libc::_exit(code);
        } else if pid > 0 {
            let mut status: libc::c_int = 0;
            if libc::waitpid(pid, &mut status, 0) < 0 {
                return Err(anyhow::anyhow!("self-probe waitpid failed"));
            }
            if !libc::WIFEXITED(status) {
                SelfProbeOutcome::ProbeInfraError("child terminated by signal")
            } else {
                classify_self_probe_exit(libc::WEXITSTATUS(status))
            }
        } else {
            return Err(anyhow::anyhow!("self-probe fork failed"));
        }
    };
    drop(owned);
    Ok(outcome)
}

/// Pure mapping of the self-probe child's exit code to an outcome. Unit-tested cross-platform.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn classify_self_probe_exit(code: i32) -> SelfProbeOutcome {
    match code {
        0 => SelfProbeOutcome::Connected,
        200 => SelfProbeOutcome::ProbeInfraError("no_new_privs_failed"),
        201 => SelfProbeOutcome::ProbeInfraError("restrict_self_failed"),
        202 => SelfProbeOutcome::ProbeInfraError("socket_failed"),
        e if e == libc::EACCES => SelfProbeOutcome::BlockedEacces,
        e => SelfProbeOutcome::OtherErrno(e),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn self_probe_denied_connect(
    _allowed_ports: &[u16],
    _deny_port: u16,
) -> anyhow::Result<SelfProbeOutcome> {
    Ok(SelfProbeOutcome::ProbeInfraError("not linux"))
}

// =============================================================================
// Non-Linux stubs (no-op)
// =============================================================================
#[cfg(not(target_os = "linux"))]
mod landlock_impl {
    /// No Landlock on non-Linux platforms.
    pub(super) fn probe_abi() -> (bool, u32) {
        (false, 0)
    }

    #[allow(dead_code)]
    pub(super) fn create_ruleset(
        _policy: &crate::policy::Policy,
        _scoped_tmp: &std::path::Path,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_probe_exit_eacces_is_blocked() {
        assert_eq!(
            classify_self_probe_exit(libc::EACCES),
            SelfProbeOutcome::BlockedEacces
        );
    }

    #[test]
    fn self_probe_exit_zero_is_connected_block_failed() {
        assert_eq!(classify_self_probe_exit(0), SelfProbeOutcome::Connected);
    }

    #[test]
    fn self_probe_exit_weak_errno_is_other_not_blocked() {
        // A weak signal must never read as a proven block.
        for e in [libc::ECONNREFUSED, libc::ETIMEDOUT, libc::ENETUNREACH] {
            assert_eq!(classify_self_probe_exit(e), SelfProbeOutcome::OtherErrno(e));
        }
    }

    #[test]
    fn self_probe_exit_sentinels_are_infra_errors_not_errno() {
        assert!(matches!(
            classify_self_probe_exit(200),
            SelfProbeOutcome::ProbeInfraError("no_new_privs_failed")
        ));
        assert!(matches!(
            classify_self_probe_exit(201),
            SelfProbeOutcome::ProbeInfraError("restrict_self_failed")
        ));
        assert!(matches!(
            classify_self_probe_exit(202),
            SelfProbeOutcome::ProbeInfraError("socket_failed")
        ));
    }

    #[test]
    fn test_detect_backend_fallback() {
        let (backend, caps) = detect_backend();
        #[cfg(not(target_os = "linux"))]
        {
            assert!(matches!(backend, BackendType::NoopAudit));
            assert!(caps.audit_only);
            assert!(!caps.enforce_fs);
            assert!(!caps.enforce_net);
        }
        // On Linux, it depends on kernel support
        let _ = (backend, caps);
    }
}
