use super::report::*;

pub fn format_text(report: &DiagnosticReport) -> String {
    let mut s = String::new();
    s.push_str(&format!("Assay v{}\n", report.assay_version));
    s.push_str("────────────\n");
    s.push_str(&format!("Platform:   {}\n", report.platform));
    if let Some(k) = &report.kernel {
        s.push_str(&format!("Kernel:     {}\n", k));
    }

    s.push_str("\nSecurity Modules:\n");
    if report.landlock.available {
        s.push_str(&format!(
            "  Landlock:   ✓ ABI v{} (FS)\n",
            report.landlock.abi_version.unwrap_or(0)
        ));
    } else {
        s.push_str("  Landlock:   ✗ unavailable\n");
    }

    if report.bpf_lsm.available {
        s.push_str("  BPF-LSM:    ✓ enabled\n");
    } else {
        s.push_str("  BPF-LSM:    ✗ unavailable\n");
    }

    s.push_str("\nHelper:\n");
    if report.helper.exists {
        s.push_str("  Status:     installed\n");
    } else {
        s.push_str("  Status:     not installed\n");
        s.push_str(&format!("  Expected:   {}\n", report.helper.path.display()));
    }

    s.push_str("\nBackend:\n");
    s.push_str(&format!(
        "  Active:     {} ({} Mode)\n",
        report.backend.selected, report.backend.mode
    ));
    s.push_str(&format!("  Details:    {}\n", report.backend.reason));

    // Phase 5 hardening features
    s.push_str("\nSandbox Hardening (v2.3):\n");
    let f = &report.sandbox_features;
    s.push_str(&format!(
        "  Env Scrubbing:      {}\n",
        if f.env_scrubbing { "✓" } else { "✗" }
    ));
    s.push_str(&format!(
        "  Scoped /tmp:        {}\n",
        if f.scoped_tmp { "✓" } else { "✗" }
    ));
    s.push_str(&format!(
        "  Fork-safe pre_exec: {}\n",
        if f.fork_safe_preexec { "✓" } else { "✗" }
    ));
    s.push_str(&format!(
        "  Deny Conflict Det:  {}\n",
        if f.deny_conflict_detection {
            "✓"
        } else {
            "✗"
        }
    ));

    s.push_str("\nStatus: ");
    match report.status {
        SystemStatus::Ready => s.push_str("✓ READY\n"),
        SystemStatus::Degraded => s.push_str("⚠ DEGRADED\n"),
        SystemStatus::Unsupported => s.push_str("✗ UNSUPPORTED\n"),
    }

    if !report.suggestions.is_empty() {
        s.push_str("\nActions:\n");
        for sugg in &report.suggestions {
            s.push_str(&format!("  → {}\n", sugg));
        }
    }

    s
}
