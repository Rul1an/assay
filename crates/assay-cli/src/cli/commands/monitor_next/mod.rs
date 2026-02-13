//! Wave3 Step2 monitor split implementation behind stable facade.
//!
//! Contract:
//! - `monitor.rs` remains the public facade.
//! - This module hosts implementation details and preserves behavior.

#[cfg(target_os = "linux")]
use crate::exit_codes;
#[cfg(target_os = "linux")]
use assay_common::encode_kernel_dev;
#[cfg(target_os = "linux")]
use futures::FutureExt;
#[cfg(target_os = "linux")]
use tokio_stream::StreamExt;

#[cfg(target_os = "linux")]
pub(crate) mod errors;
#[cfg(target_os = "linux")]
pub(crate) mod events;
#[cfg(target_os = "linux")]
pub(crate) mod normalize;
#[cfg(target_os = "linux")]
pub(crate) mod output;
#[cfg(target_os = "linux")]
pub(crate) mod rules;
#[cfg(target_os = "linux")]
pub(crate) mod syscall_linux;
pub(crate) mod tests;

pub(crate) async fn run(args: super::MonitorArgs) -> anyhow::Result<i32> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        eprintln!("Error: 'assay monitor' is only supported on Linux.");
        Ok(40)
    }

    #[cfg(target_os = "linux")]
    {
        run_linux(args).await
    }
}

#[cfg(target_os = "linux")]
async fn run_linux(args: super::MonitorArgs) -> anyhow::Result<i32> {
    use assay_common::{get_inode_generation, strict_open};
    use assay_monitor::Monitor;

    let mut runtime_config = None;
    let mut kill_config = None;
    if let Some(path) = &args.policy {
        let p = assay_core::mcp::policy::McpPolicy::from_file(path)?;
        if let Some(rm) = p.runtime_monitor {
            if !rm.enabled {
                if !args.quiet {
                    eprintln!("Runtime monitor disabled by policy.");
                }
                return Ok(0);
            }
            runtime_config = Some(rm);
        }
        kill_config = p.kill_switch;
    }

    let ebpf_path = match args.ebpf.as_ref() {
        Some(p) => p.clone(),
        None => std::path::PathBuf::from("target/assay-ebpf.o"),
    };

    if !ebpf_path.exists() {
        eprintln!("Error: eBPF object not found at {}. Build it with 'cargo xtask build-ebpf' or provide --ebpf <path>", ebpf_path.display());
        return Ok(40);
    }

    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to load eBPF: {}", e);
            return Ok(40);
        }
    };

    if args.monitor_all {
        if !args.quiet {
            println!("⚠️  MONITOR_ALL enabled: Bypassing Cgroup filtering.");
        }
        monitor.set_monitor_all(true)?;

        let v = monitor.get_config_u32(assay_common::KEY_MONITOR_ALL)?;
        println!(
            "DEBUG: CONFIG[{}]={} confirmed",
            assay_common::KEY_MONITOR_ALL,
            v
        );
        if v != 1 {
            eprintln!(
                "❌ Failed to enable MONITOR_ALL (CONFIG[{}] != 1)",
                assay_common::KEY_MONITOR_ALL
            );
            return Ok(40);
        }
    }

    if !args.pid.is_empty() {
        if let Err(e) = monitor.set_monitored_pids(&args.pid) {
            eprintln!("Warning: Failed to populate PID map: {}", e);
        }

        let mut cgroups = Vec::new();
        for &pid in &args.pid {
            match normalize::resolve_cgroup_id(pid) {
                Ok(id) => cgroups.push(id),
                Err(e) => eprintln!("Warning: Failed to resolve cgroup for PID {}: {}", pid, e),
            }
        }

        if !cgroups.is_empty() {
            if let Err(e) = monitor.set_monitored_cgroups(&cgroups) {
                eprintln!("Error: Failed to populate Cgroup map: {}", e);
                return Ok(40);
            }
            if !args.quiet {
                eprintln!("Monitored Cgroups: {:?}", cgroups);
            }
        } else {
            eprintln!("Warning: No valid cgroups resolved. Rules will not match.");
        }
    }

    if let Err(e) = monitor.attach() {
        eprintln!("Failed to attach probes: {}", e);
        return Ok(40);
    }

    if !args.quiet {
        eprintln!("Assay Monitor running. Press Ctrl-C to stop.");
        if !args.pid.is_empty() {
            eprintln!("Monitoring PIDs: {:?}", args.pid);
        }
    }

    let rules = rules::compile_active_rules(runtime_config.as_ref());

    if let Some(cfg) = &runtime_config {
        let mut t1_policy = assay_policy::tiers::Policy::default();

        for r in &cfg.rules {
            let is_enforcement = matches!(
                r.action,
                assay_core::mcp::runtime_features::MonitorAction::TriggerKill
                    | assay_core::mcp::runtime_features::MonitorAction::Deny
            );

            if !is_enforcement {
                continue;
            }

            match r.rule_type {
                assay_core::mcp::runtime_features::MonitorRuleType::FileOpen => {
                    for glob in &r.match_config.path_globs {
                        t1_policy.files.deny.push(glob.clone());
                    }
                    if let Some(not) = &r.match_config.not {
                        for glob in &not.path_globs {
                            t1_policy.files.allow.push(glob.clone());
                        }
                    }
                }
                assay_core::mcp::runtime_features::MonitorRuleType::NetConnect => {
                    for dest in &r.match_config.dest_globs {
                        let is_cidr = if let Some((ip_part, prefix_part)) = dest.split_once('/') {
                            ip_part.parse::<std::net::IpAddr>().is_ok()
                                && prefix_part.parse::<u8>().is_ok()
                        } else {
                            false
                        };

                        if is_cidr {
                            t1_policy.network.deny_cidrs.push(dest.clone());
                        } else if let Ok(port) = dest.parse::<u16>() {
                            t1_policy.network.deny_ports.push(port);
                        } else {
                            t1_policy.network.deny_destinations.push(dest.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        let mut compiled = assay_policy::tiers::compile(&t1_policy);

        let mut inode_rules = Vec::with_capacity(compiled.tier1.file_deny_exact.len());

        for rule in &compiled.tier1.file_deny_exact {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            let c_path = match CString::new(std::path::Path::new(&rule.path).as_os_str().as_bytes())
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Warning: Invalid path encoding {} ({})", rule.path, e);
                    continue;
                }
            };

            let guard_fd_res = strict_open::openat2_strict(&c_path);
            let guard_fd = match guard_fd_res {
                Ok(fd) => fd,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::Unsupported
                        || e.raw_os_error() == Some(libc::ENOSYS)
                    {
                        eprintln!(
                            "Warning: Strict open (openat2) unavailable on this system, using O_PATH fallback for {}",
                            rule.path
                        );
                        match syscall_linux::open_path_no_symlink(&c_path) {
                            Ok(fd) => fd,
                            Err(err) => {
                                eprintln!(
                                    "Warning: Fallback open failed for {}: {}",
                                    rule.path, err
                                );
                                continue;
                            }
                        }
                    } else if e.raw_os_error() == Some(libc::ELOOP)
                        || e.raw_os_error() == Some(libc::EXDEV)
                    {
                        eprintln!("Warning: Strict open blocked access to {} (Symlink/Breakout detected): {}", rule.path, e);
                        continue;
                    } else {
                        eprintln!("Warning: Failed to open denied path {}: {}", rule.path, e);
                        continue;
                    }
                }
            };

            let stat = match syscall_linux::fstat_fd(guard_fd) {
                Ok(stat) => stat,
                Err(e) => {
                    syscall_linux::close_fd(guard_fd);
                    eprintln!(
                        "Warning: Could not fstat denied path {} (skipping): {}",
                        rule.path, e
                    );
                    continue;
                }
            };

            let gen = match get_inode_generation(guard_fd) {
                Ok(g) => g,
                Err(e) => {
                    let eno = e.raw_os_error().unwrap_or(0);
                    if eno == libc::ENOTTY || eno == libc::EINVAL {
                        0
                    } else {
                        eprintln!(
                            "Warning: Could not get inode generation for {} (using gen=0): {}",
                            rule.path, e
                        );
                        0
                    }
                }
            };

            syscall_linux::close_fd(guard_fd);

            let dev = stat.st_dev;
            let ino = stat.st_ino;
            let kernel_dev = encode_kernel_dev(dev);

            if !args.quiet {
                let maj = libc::major(stat.st_dev);
                let min = libc::minor(stat.st_dev);
                eprintln!(
                    "Matched Inode for {}: dev={} (maj={}, min={}) -> kernel_dev={} ino={} gen={}",
                    rule.path, dev, maj, min, kernel_dev, ino, gen
                );
            }

            inode_rules.push(assay_policy::tiers::InodeRule {
                rule_id: rule.rule_id,
                dev: kernel_dev,
                ino,
                gen,
            });
        }

        compiled.tier1.inode_deny_exact.extend(inode_rules);

        if !args.quiet {
            eprintln!("Locked & Loaded Assurance Policy 🛡️");
            eprintln!("  • Tier 1 (Kernel): {} rules", compiled.stats.tier1_rules);
            eprintln!("  • Tier 2 (User):   {} rules", compiled.stats.tier2_rules);
            if !compiled.stats.warnings.is_empty() {
                for w in &compiled.stats.warnings {
                    eprintln!("    ⚠️  {}", w);
                }
            }
        }

        if let Err(e) = monitor.set_tier1_rules(&compiled) {
            eprintln!(
                "Warning: Failed to load Tier 1 rules (LSM might be unavailable): {}",
                e
            );
        }
    }

    let mut stream = monitor.listen().map_err(|e| anyhow::anyhow!(e))?;

    let mut timeout = match args.duration {
        Some(d) => tokio::time::sleep(d.into()).boxed(),
        None => std::future::pending().boxed(),
    };

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                if !args.quiet { eprintln!("\nStopping monitor..."); }
                break;
            }
            _ = &mut timeout => {
                if !args.quiet { eprintln!("\nDuration expired."); }
                break;
            }
            event_res = stream.next() => {
                match event_res {
                    Some(Ok(event)) => {
                        events::handle_event(&event, &args, &rules, kill_config.as_ref()).await;
                    }
                    Some(Err(e)) => {
                        eprintln!("Monitor stream error: {}", e);
                    }
                    None => {
                        eprintln!("Stream channel closed.");
                        break;
                    }
                }
            }
        }
    }

    Ok(exit_codes::OK)
}
