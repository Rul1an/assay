use super::report::*;
use std::path::PathBuf;

pub fn probe_system() -> DiagnosticReport {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let platform = format!("{} {}", os, arch);

    let kernel = probe_kernel();
    let lsms = probe_lsms();
    let landlock = probe_landlock(&lsms);
    let bpf_lsm = probe_bpf_lsm(&lsms);
    let helper = probe_helper();

    // Backend Selection Logic (reuse assay-cli backend logic)
    let (backend_impl, caps) = crate::backend::detect_backend();
    let backend_name = backend_impl.name();

    // Determine mode
    let mode = if caps.enforce_fs {
        "Enforcement"
    } else {
        "Containment"
    };

    // Determine status
    let status = if os != "linux" {
        SystemStatus::Unsupported
    } else {
        match backend_name {
            "BPF-LSM" => SystemStatus::Ready,
            "Landlock" => SystemStatus::Degraded,
            _ => SystemStatus::Degraded,
        }
    };

    let reason = if backend_name == "Landlock" && !helper.exists {
        "Landlock available; BPF helper not installed".to_string()
    } else if backend_name == "NoopAudit" {
        "No supported containment features".to_string()
    } else {
        match status {
            SystemStatus::Ready => "System fully supported".to_string(),
            SystemStatus::Degraded => "Running with reduced capabilities".to_string(),
            SystemStatus::Unsupported => "Platform not supported".to_string(),
        }
    };

    let backend = BackendSelection {
        selected: backend_name.to_string(),
        mode: mode.to_string(),
        reason,
    };

    let mut suggestions = Vec::new();
    if status == SystemStatus::Degraded && os == "linux" {
        suggestions.push("Run `assay setup --apply` to install privileged helper".to_string());
    }

    DiagnosticReport {
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        platform,
        kernel,
        lsms,
        landlock,
        bpf_lsm,
        helper,
        backend,
        sandbox_features: SandboxFeatures::default(),
        metrics: crate::metrics::get_all(),
        status,
        suggestions,
    }
}

fn probe_kernel() -> Option<String> {
    if cfg!(target_os = "linux") {
        std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

fn probe_lsms() -> Vec<String> {
    if let Ok(c) = std::fs::read_to_string("/sys/kernel/security/lsm") {
        c.trim().split(',').map(String::from).collect()
    } else {
        Vec::new()
    }
}

fn probe_landlock(lsms: &[String]) -> LandlockStatus {
    // LSM-list membership is an extra observation only. The source of truth for ABI and net support
    // is the landlock_create_ruleset(NULL, 0, VERSION) syscall: the sysfs path the old probe read
    // (/sys/kernel/security/landlock/abi_version) does not exist on mainline kernels and produced a
    // false-negative net_enforce on real hosts (e.g. Ubuntu 24.04, kernel 6.8, ABI 4).
    let available = lsms.contains(&"landlock".to_string());

    let (abi_probe_status, abi_version, abi_probe_errno) = query_landlock_abi();
    let net_connect_tcp_supported = net_connect_tcp_supported(abi_version);
    let net_bind_tcp_supported = net_bind_tcp_supported(abi_version);
    let no_new_privs_settable = probe_no_new_privs_settable();
    let (net_connect_ruleset_probe, net_connect_ruleset_errno) =
        super::landlock_net_smoke::probe_net_connect_ruleset(abi_version);

    LandlockStatus {
        available,
        fs_enforce: available,
        net_enforce: net_connect_tcp_supported,
        abi_version,
        abi_version_source: if abi_version.is_some() {
            "landlock_create_ruleset_version"
        } else {
            "none"
        },
        abi_probe_status,
        abi_probe_errno,
        net_connect_tcp_supported,
        net_bind_tcp_supported,
        no_new_privs_settable,
        net_connect_ruleset_probe,
        net_connect_ruleset_errno,
    }
}

/// Classify the raw return of `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`.
/// Pure logic, unit-tested cross-platform; the actual syscall is Linux-only (below), so on non-Linux
/// builds this is only reachable from tests.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn classify_abi(ret: i64, errno: i32) -> (LandlockAbiProbeStatus, Option<u32>, Option<i32>) {
    if ret >= 1 {
        (LandlockAbiProbeStatus::Ok, Some(ret as u32), None)
    } else if errno == libc::ENOSYS {
        (LandlockAbiProbeStatus::Unsupported, None, Some(errno))
    } else if errno == libc::EOPNOTSUPP {
        (LandlockAbiProbeStatus::Disabled, None, Some(errno))
    } else {
        (LandlockAbiProbeStatus::Error, None, Some(errno))
    }
}

/// `LANDLOCK_ACCESS_NET_CONNECT_TCP` is available from ABI 4 (the future Plan 2A connect path).
fn net_connect_tcp_supported(abi: Option<u32>) -> bool {
    abi.is_some_and(|v| v >= 4)
}

/// `LANDLOCK_ACCESS_NET_BIND_TCP` is also available from ABI 4.
fn net_bind_tcp_supported(abi: Option<u32>) -> bool {
    abi.is_some_and(|v| v >= 4)
}

// The ABI query and the no_new_privs probe are raw syscalls. assay-cli already opts into unsafe
// (`#![allow(unsafe_code)]` in main.rs) for its sandbox path, and backend.rs's enforce() already runs
// raw prctl + landlock_restrict_self in pre_exec, so this is consistent with the crate's existing
// policy rather than a new unsafe surface. The landlock crate gives an ABI number (see
// backend.rs::detect_backend / caps.abi_version) but caps probing at V4 and does not expose the
// ENOSYS-vs-EOPNOTSUPP distinction, so the canonical VERSION syscall is used directly here.
#[cfg(target_os = "linux")]
fn query_landlock_abi() -> (LandlockAbiProbeStatus, Option<u32>, Option<i32>) {
    // landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION) returns the ABI version.
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_long = 1;
    // SAFETY: a pure version query — NULL attr, 0 size, VERSION flag. Reads no memory, restricts
    // nothing, has no side effects on this process.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    let errno = if ret < 0 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    } else {
        0
    };
    classify_abi(ret as i64, errno)
}

#[cfg(not(target_os = "linux"))]
fn query_landlock_abi() -> (LandlockAbiProbeStatus, Option<u32>, Option<i32>) {
    // No Landlock off Linux.
    (LandlockAbiProbeStatus::Unsupported, None, None)
}

/// Whether `PR_SET_NO_NEW_PRIVS` can be set, measured in a throwaway forked child so the diagnostics
/// process itself is never permanently put into no-new-privs. Only async-signal-safe calls (prctl,
/// _exit) run between fork and exit.
#[cfg(target_os = "linux")]
fn probe_no_new_privs_settable() -> bool {
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    const PR_GET_NO_NEW_PRIVS: libc::c_int = 39;
    // SAFETY: child runs only prctl + _exit (async-signal-safe); parent only waitpid.
    unsafe {
        let pid = libc::fork();
        if pid == 0 {
            let set = libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            let get = libc::prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0);
            libc::_exit(if set == 0 && get == 1 { 0 } else { 1 });
        } else if pid > 0 {
            let mut status: libc::c_int = 0;
            if libc::waitpid(pid, &mut status, 0) < 0 {
                return false;
            }
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0
        } else {
            false
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn probe_no_new_privs_settable() -> bool {
    false
}

fn probe_bpf_lsm(lsms: &[String]) -> BpfLsmStatus {
    BpfLsmStatus {
        available: lsms.contains(&"bpf".to_string()),
    }
}

fn probe_helper() -> HelperStatus {
    let path = PathBuf::from("/usr/local/bin/assay-bpf");
    let socket = PathBuf::from("/run/assay/bpf.sock");

    HelperStatus {
        exists: path.exists(),
        path,
        version: None,
        socket_exists: socket.exists(),
        socket,
    }
}

#[cfg(test)]
mod landlock_probe_tests {
    use super::*;

    #[test]
    fn abi_ok_returns_version_and_no_errno() {
        assert_eq!(
            classify_abi(4, 0),
            (LandlockAbiProbeStatus::Ok, Some(4), None)
        );
        assert_eq!(
            classify_abi(1, 0),
            (LandlockAbiProbeStatus::Ok, Some(1), None)
        );
    }

    #[test]
    fn abi_enosys_is_unsupported() {
        assert_eq!(
            classify_abi(-1, libc::ENOSYS),
            (
                LandlockAbiProbeStatus::Unsupported,
                None,
                Some(libc::ENOSYS)
            )
        );
    }

    #[test]
    fn abi_eopnotsupp_is_disabled() {
        assert_eq!(
            classify_abi(-1, libc::EOPNOTSUPP),
            (
                LandlockAbiProbeStatus::Disabled,
                None,
                Some(libc::EOPNOTSUPP)
            )
        );
    }

    #[test]
    fn abi_other_errno_is_error_with_errno() {
        assert_eq!(
            classify_abi(-1, libc::EPERM),
            (LandlockAbiProbeStatus::Error, None, Some(libc::EPERM))
        );
    }

    #[test]
    fn net_caps_require_abi_4() {
        assert!(!net_connect_tcp_supported(None));
        assert!(!net_connect_tcp_supported(Some(3)));
        assert!(net_connect_tcp_supported(Some(4)));
        assert!(net_connect_tcp_supported(Some(5)));
        assert!(!net_bind_tcp_supported(Some(3)));
        assert!(net_bind_tcp_supported(Some(4)));
    }

    #[test]
    fn no_new_privs_probe_returns_without_panic() {
        // We cannot assert the value cross-platform (true on Linux, false elsewhere), only that the
        // forked-child probe returns a bool and never panics.
        let _: bool = probe_no_new_privs_settable();
    }

    #[test]
    fn report_is_additive_and_keeps_old_fields() {
        let status = probe_landlock(&[]);
        let v = serde_json::to_value(&status).unwrap();
        // Old fields still present.
        for k in ["available", "fs_enforce", "net_enforce", "abi_version"] {
            assert!(v.get(k).is_some(), "missing pre-existing field {k}");
        }
        // New preflight fields present.
        for k in [
            "abi_version_source",
            "abi_probe_status",
            "abi_probe_errno",
            "net_connect_tcp_supported",
            "net_bind_tcp_supported",
            "no_new_privs_settable",
            "net_connect_ruleset_probe",
            "net_connect_ruleset_errno",
        ] {
            assert!(v.get(k).is_some(), "missing new field {k}");
        }
    }
}
