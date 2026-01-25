//! Linux Capability probing (file-based).

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct CapReport {
    pub cap_bpf: bool,
    pub cap_perfmon: bool,
    pub cap_sys_resource: bool,
}

/// Check file capabilities (Linux only).
/// Returns default (false) on other platforms.
pub fn file_has_caps(path: &Path) -> anyhow::Result<CapReport> {
    #[cfg(target_os = "linux")]
    {
        // Ideally we use a crate like `caps` or `capng`.
        // For v0.1 without adding deps, we can parse `getcap` output if available.
        // Or we can rely on `setup status` heuristics.
        // Let's implement a `getcap` parser as a robust fallback.

        let output = std::process::Command::new("getcap").arg(path).output();

        match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                // Output format: /path/to/file = cap_bpf,cap_perfmon+ep
                Ok(CapReport {
                    cap_bpf: s.contains("cap_bpf"),
                    cap_perfmon: s.contains("cap_perfmon"),
                    cap_sys_resource: s.contains("cap_sys_resource"),
                })
            }
            Err(_) => {
                // getcap not found or fail
                Ok(CapReport::default())
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Ok(CapReport::default())
    }
}
