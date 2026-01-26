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
    let available = lsms.contains(&"landlock".to_string());

    // Read actual ABI version from sysfs (PR5.5 enhancement)
    let abi_version = if available {
        std::fs::read_to_string("/sys/kernel/security/landlock/abi_version")
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    } else {
        None
    };

    // Net enforcement requires ABI >= 4
    let net_enforce = abi_version.map(|v| v >= 4).unwrap_or(false);

    LandlockStatus {
        available,
        fs_enforce: available,
        net_enforce,
        abi_version,
    }
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
