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
    pub audit_only: bool,
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
    if landlock_impl::probe_fs() {
        (
            BackendType::Landlock,
            BackendCaps {
                enforce_fs: true,
                enforce_net: false, // NET not implemented yet - honest reporting
                audit_only: false,
            },
        )
    } else {
        // Fallback: no enforcement (macOS, old Linux, etc.)
        (
            BackendType::NoopAudit,
            BackendCaps {
                enforce_fs: false,
                enforce_net: false,
                audit_only: true,
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
    /// Enforce the ruleset. Safe to call from pre_exec (no allocations on Linux).
    pub fn enforce(&mut self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(ruleset) = self.ruleset.take() {
            return landlock_impl::enforce(ruleset).map_err(std::io::Error::other);
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

    /// Probe-based Landlock FS detection.
    pub(super) fn probe_fs() -> bool {
        // Actually try to create a minimal ruleset to verify kernel support
        match Ruleset::default().handle_access(AccessFs::from_all(ABI::V1)) {
            Ok(ruleset) => ruleset.create().is_ok(),
            Err(_) => false,
        }
    }

    pub(super) type RulesetHandle = RulesetCreated;

    pub(super) fn enforce(ruleset: RulesetHandle) -> anyhow::Result<()> {
        let status = ruleset.restrict_self()?;
        if status.ruleset == RulesetStatus::NotEnforced {
            // This is arguably safe to construct in pre_exec as it's just an error
            // But ideally we'd avoid even this allocation.
            // For now, let's allow it as a rare error case.
            anyhow::bail!("Landlock not enforced (kernel may not support it)");
        }
        Ok(())
    }

    /// Create Landlock ruleset from policy.
    pub(super) fn create_ruleset(
        policy: &crate::policy::Policy,
        scoped_tmp: &Path,
    ) -> anyhow::Result<RulesetCreated> {
        let abi = ABI::V1;
        let mut ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(abi))?
            .create()?;

        // 1. Allow CWD (RX only by default, safe containment)
        if let Ok(cwd) = std::env::current_dir() {
            ruleset = add_path(
                ruleset,
                cwd.to_string_lossy().as_ref(),
                AccessFs::from_read(abi) | AccessFs::Execute,
            )?;
        }

        // 2. Allow scoped /tmp (RWX)
        // Ensure it exists? Caller responsibility, but we check existence in add_path.
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
        // Note: We assume policy has already been filtered for deny rules (Policy Engine responsibility).
        if let Ok(_cwd) = std::env::current_dir() {
            for path in &policy.fs.allow {
                let expanded = expand_path(path);

                // Grant RWX only if specifically allowed.
                // Narrowing logic: checks if under CWD or tmp, but for now we trust explicit allows.
                // Default CWD is RX, so explicit allow on ${CWD}/out is needed for write.
                let access = AccessFs::from_all(abi);
                ruleset = add_path(ruleset, &expanded, access)?;
            }
        } else {
            for path in &policy.fs.allow {
                let expanded = expand_path(path);
                ruleset = add_path(ruleset, &expanded, AccessFs::from_all(abi))?;
            }
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
    pub(super) fn probe_fs() -> bool {
        false
    }

    /// No-op on non-Linux platforms.
    /// No-op on non-Linux platforms.
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
