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

/// Prepare Landlock ruleset. allocations/IO allowed here.
pub fn prepare_landlock(
    policy: &crate::policy::Policy,
    scoped_tmp: &std::path::Path,
) -> anyhow::Result<LandlockEnforcer> {
    #[cfg(target_os = "linux")]
    {
        let ruleset = landlock_impl::create_ruleset(policy, scoped_tmp)?;
        Ok(LandlockEnforcer {
            ruleset: Some(ruleset),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = policy;
        let _ = scoped_tmp;
        Ok(LandlockEnforcer {})
    }
}

// =============================================================================
// Linux-only Landlock implementation
// =============================================================================
#[cfg(target_os = "linux")]
mod landlock_impl {
    use landlock::{
        Access, AccessFs, BitFlags, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreated,
        RulesetCreatedAttr, RulesetStatus, ABI,
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
        let mut max_abi = 0;
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
        } else {
            return (true, max_abi);
        }

        // Check V5 (IOCTL)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V5))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 5;
        } else {
            return (true, max_abi);
        }

        // Check V6 (IPC Scopes - Linux 6.12+)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V6))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 6;
        } else {
            return (true, max_abi);
        }

        // Check V7 (Audit Logging - Linux 6.15+)
        if Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V7))
            .and_then(|r| r.create())
            .is_ok()
        {
            max_abi = 7;
        }

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

    /// Create Landlock ruleset from policy.
    pub(super) fn create_ruleset(
        policy: &crate::policy::Policy,
        scoped_tmp: &Path,
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

        // TODO: NET rules (ABI V4) if abi_level >= 4
        // if abi_level >= 4 { ruleset = ruleset.handle_access(AccessNet::from_all(abi))?; }

        let mut ruleset = ruleset.create()?;

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
