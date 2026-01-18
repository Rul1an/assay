
#[cfg(target_os = "linux")]
use crate::cli::commands::exit_codes;
use clap::Args;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use tokio_stream::StreamExt;

#[derive(Args, Debug, Clone)]
#[command(
    about = "Runtime eBPF monitor (Linux only, experimental)",
    long_about = "Runtime eBPF monitor (Linux only, experimental)\n\
\n\
Requirements:\n\
  • Linux kernel with eBPF support\n\
  • Privileges: root or CAP_BPF + CAP_PERFMON (and often CAP_SYS_ADMIN depending on distro)\n\
\n\
Notes:\n\
  • Experimental: events may be dropped under load (ring buffer backpressure)\n\
  • Non-Linux: exits with code 40 (MONITOR_ATTACH_FAILED / NotSupported)\n",
    after_help = "Examples:\n\
  # Build eBPF bytecode\n\
  cargo xtask build-ebpf\n\
\n\
  # Monitor a process for 60s\n\
  sudo assay monitor --pid 1234 --duration 60s\n\
\n\
  # Explicit eBPF path\n\
  sudo assay monitor --ebpf target/assay-ebpf.o --pid 1234\n\
\n\
Common failures:\n\
  • EPERM: run with sudo or grant CAP_BPF/CAP_PERFMON\n\
  • Missing artifact: run `cargo xtask build-ebpf` (expects target/assay-ebpf.o)\n"
)]
pub struct MonitorArgs {
    /// PIDs to monitor (comma separated or multiple flags)
    #[arg(short, long)]
    pub pid: Vec<u32>,

    /// Path to eBPF object file. If not provided, defaults to local artifact if present.
    #[arg(long)]
    pub ebpf: Option<PathBuf>,

    /// Print events to stdout (implied if no other output specified)
    #[arg(long)]
    pub print: bool,

    /// Suppress all event output (only errors/stats)
    #[arg(long)]
    pub quiet: bool,

    /// Duration to run (e.g. "60s"). If omitted, runs until Ctrl-C.
    #[arg(long)]
    pub duration: Option<humantime::Duration>,

    /// Policy file to enable runtime enforcement rules
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Monitor ALL cgroups (bypass filtering, useful for debugging)
    #[arg(long)]
    pub monitor_all: bool,
}


pub async fn run(args: MonitorArgs) -> anyhow::Result<i32> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        eprintln!("Error: 'assay monitor' is only supported on Linux.");
        Ok(40) // MONITOR_NOT_SUPPORTED code
    }

    #[cfg(target_os = "linux")]
    {
        run_linux(args).await
    }
}

#[cfg(target_os = "linux")]
async fn run_linux(args: MonitorArgs) -> anyhow::Result<i32> {
    use assay_monitor::Monitor;
    use assay_common::{EVENT_OPENAT, EVENT_CONNECT};

    let mut runtime_config = None;
    let mut kill_config = None;
    if let Some(path) = &args.policy {
        let p = assay_core::mcp::policy::McpPolicy::from_file(path)?;
        if let Some(rm) = p.runtime_monitor {
            if !rm.enabled {
                if !args.quiet { eprintln!("Runtime monitor disabled by policy."); }
                return Ok(0);
            }
            runtime_config = Some(rm);
        }
        kill_config = p.kill_switch;
    }

    let ebpf_path = match args.ebpf {
        Some(p) => p,
        None => {
            // Heuristic: default to the output location of `cargo xtask build-ebpf`
            // User can override with --ebpf
            PathBuf::from("target/assay-ebpf.o")
        }
    };

    if !ebpf_path.exists() {
        eprintln!("Error: eBPF object not found at {}. Build it with 'cargo xtask build-ebpf' or provide --ebpf <path>", ebpf_path.display());
        return Ok(40);
    }

    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to load eBPF: {}", e);
            return Ok(40); // MONITOR_ATTACH_FAILED
        }
    };

    if !args.pid.is_empty() {
        // Compatibility: Populate Legacy PID Map (for Tracepoints if they use it)
        if let Err(e) = monitor.set_monitored_pids(&args.pid) {
             eprintln!("Warning: Failed to populate PID map: {}", e);
        }

        // Resolve Cgroup IDs for PIDs and populate MONITORED_CGROUPS
        let mut cgroups = Vec::new();
        for &pid in &args.pid {
            // Cgroup V2 ID Resolution
            // ID = Inode of /sys/fs/cgroup/unified/<path> or just /proc/<pid>/cgroup path mapping
            // Robust way: readlink /proc/<pid>/ns/cgroup? No, that's namespace.
            // Correct way: open /proc/<pid>/cgroup, parse 0::/path, stat /sys/fs/cgroup/path

            // Simplified for verification (assuming /sys/fs/cgroup mount):
            match resolve_cgroup_id(pid) {
                Ok(id) => cgroups.push(id),
                Err(e) => eprintln!("Warning: Failed to resolve cgroup for PID {}: {}", pid, e),
            }
        }

        if !cgroups.is_empty() {
             if let Err(e) = monitor.set_monitored_cgroups(&cgroups) {
                 eprintln!("Error: Failed to populate Cgroup map: {}", e);
                 return Ok(40);
             }
             if !args.quiet { eprintln!("Monitored Cgroups: {:?}", cgroups); }
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

    #[cfg(target_os = "linux")]
    #[derive(Debug)]
    struct ActiveRule {
        id: String,
        action: assay_core::mcp::runtime_features::MonitorAction,

        // NOTE: Despite the name, `allow` represents the set of glob patterns whose
        // matches cause this rule to fire (i.e., be treated as a violation). A path
        // that matches `allow` and is *not* matched by `deny` will trigger the rule.
        allow: globset::GlobSet,
        deny: Option<globset::GlobSet>, // for match.not
    }

    #[cfg(target_os = "linux")]
    fn compile_globset(globs: &[String]) -> anyhow::Result<globset::GlobSet> {
        let mut b = globset::GlobSetBuilder::new();
        for g in globs {
            b.add(globset::Glob::new(g)?);
        }
        Ok(b.build()?)
    }

    #[cfg(target_os = "linux")]
    fn normalize_path_syntactic(input: &str) -> String {
        // Syntactic normalization (no filesystem canonicalize -> less TOCTOU)
        // - collapse '//' -> '/'
        // - remove '/./'
        // - resolve '/../' in-place
        let is_absolute = input.starts_with('/');
        let mut parts = Vec::new();
        for part in input.split('/') {
            match part {
                "" | "." => {}
                ".." => { parts.pop(); }
                x => parts.push(x),
            }
        }
        if is_absolute {
            if parts.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", parts.join("/"))
            }
        } else {
            parts.join("/")
        }
    }

    #[cfg(target_os="linux")]
    async fn kill_pid(pid: u32, mode: assay_core::mcp::runtime_features::KillMode, grace_ms: u64) {
        unsafe {
            libc::kill(pid as i32, if mode == assay_core::mcp::runtime_features::KillMode::Immediate { libc::SIGKILL } else { libc::SIGTERM });
        }
        if mode == assay_core::mcp::runtime_features::KillMode::Graceful {
            tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;
            unsafe { libc::kill(pid as i32, libc::SIGKILL); }
        }
    }

    let mut rules = Vec::new();
    if let Some(cfg) = &runtime_config {
        for r in &cfg.rules {
            let kind = r.rule_type.clone();
            let mc = &r.match_config;

            // support only file_open for now (openat)
            // Note: need to import MonitorRuleType or use full path
            if !matches!(kind, assay_core::mcp::runtime_features::MonitorRuleType::FileOpen) {
                continue;
            }

            match compile_globset(&mc.path_globs) {
                Ok(allow) => {
                     let deny = mc.not.as_ref().map(|n| compile_globset(&n.path_globs)).transpose().unwrap_or(None);
                     rules.push(ActiveRule {
                        id: r.id.clone(),
                        action: r.action.clone(),
                        allow,
                        deny,
                    });
                }
                Err(e) => {
                     eprintln!("Warning: Failed to compile glob for rule {}: {}", r.id, e);
                }
            }
        }
    }

    // --- Tier 1 Policy Compilation ---
    if let Some(cfg) = &runtime_config {
        let mut t1_policy = assay_policy::tiers::Policy::default();

        for r in &cfg.rules {
            // Assume TriggerKill means "Block if possible"
            // If action is Log/Alert, technically we shouldn't block kernel-side.
            // But for "Shield" implementation, we usually map "Deny/Kill" rules matchers.
            // The McpPolicy separates "Action" from "Rule".
            // If action is Log, we shouldn't put it in Tier 1 Deny.
            let is_enforcement = matches!(r.action,
                assay_core::mcp::runtime_features::MonitorAction::TriggerKill |
                assay_core::mcp::runtime_features::MonitorAction::Deny
            );

            if !is_enforcement { continue; }

            match r.rule_type {
                assay_core::mcp::runtime_features::MonitorRuleType::FileOpen => {
                     // Map to file deny
                     for glob in &r.match_config.path_globs {
                         t1_policy.files.deny.push(glob.clone());
                     }
                     // Map exceptions
                     if let Some(not) = &r.match_config.not {
                         for glob in &not.path_globs {
                             t1_policy.files.allow.push(glob.clone());
                         }
                     }
                }
                assay_core::mcp::runtime_features::MonitorRuleType::NetConnect => {
                    for dest in &r.match_config.dest_globs {
                        // Attempt to parse as CIDR, otherwise Glob
                        // Heuristic: Check if "IP/Prefix" format
                        let is_cidr = if let Some((ip_part, prefix_part)) = dest.split_once('/') {
                             // Check if left side is IP and right side is number
                             ip_part.parse::<std::net::IpAddr>().is_ok() && prefix_part.parse::<u8>().is_ok()
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

        let compiled = assay_policy::tiers::compile(&t1_policy);

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
            eprintln!("Warning: Failed to load Tier 1 rules (LSM might be unavailable): {}", e);
        }
    }

    if args.monitor_all {
        if !args.quiet { println!("⚠️  MONITOR_ALL enabled: Bypassing Cgroup filtering."); }
        monitor.set_monitor_all(true)?;
    }

    let mut stream = monitor.listen().map_err(|e| anyhow::anyhow!(e))?;

    // Ctrl-C handler
    let mut timeout = match args.duration {
        Some(d) => tokio::time::sleep(d.into()).boxed(),
        None => std::future::pending().boxed(),
    };

    fn decode_utf8_cstr(data: &[u8]) -> String {
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        String::from_utf8_lossy(&data[..end]).to_string()
    }

    fn dump_prefix_hex(data: &[u8], n: usize) -> String {
        data.iter().take(n).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("")
    }

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
                        let mut violation_rule: Option<&ActiveRule> = None;

                        // ENFORCEMENT LOGIC (Linux Only) - Tier 2
                        if !rules.is_empty() && event.event_type == EVENT_OPENAT {
                             let raw = decode_utf8_cstr(&event.data);
                             let path = normalize_path_syntactic(&raw);

                             for r in &rules {
                                 if r.allow.is_match(&path) {
                                     // Check deny list (not)
                                     let blocked = r.deny.as_ref().map(|d| d.is_match(&path)).unwrap_or(false);
                                     if !blocked {
                                         violation_rule = Some(r);
                                         break; // First match triggers
                                     }
                                 }
                             }
                        }

                        // REACT (Tier 2)
                        // Note: If Kernel blocked it (Tier 1), we get EVENT_FILE_BLOCKED instead of openat?
                        // Actually, lsm_file_open returns EPERM.
                        // Does tracepoint sys_enter_openat still fire? Yes.
                        // So we might technically double-match here.
                        // But since it's already blocked, 'kill' is redundant but harmless (process might be handling error).
                        // If we want to avoid killing blocked processes, we need coordination.
                        // But for now, rigorous enforcement is safer.

                        if let Some(rule) = violation_rule {
                             if !args.quiet {
                                 println!("[PID {}] 🚨 VIOLATION: Rule '{}' matched file access", event.pid, rule.id);
                             }

                             if rule.action == assay_core::mcp::runtime_features::MonitorAction::TriggerKill {
                                 // Check Kill Switch Configuration
                                 let default_mode = assay_core::mcp::runtime_features::KillMode::Graceful;
                                 let default_grace = 3000;

                                 let (enabled, mode, grace) = if let Some(kc) = &kill_config {
                                     let trigger = kc.triggers.iter().find(|t| t.on_rule == rule.id);
                                     let mode = trigger.and_then(|t| t.mode.clone()).unwrap_or(kc.mode.clone());
                                     (kc.enabled, mode, kc.grace_period_ms)
                                 } else {
                                     (false, default_mode, default_grace)
                                 };

                                 if enabled {
                                      if !args.quiet { println!("[PID {}] 💀 INIT KILL (mode={:?}, grace={}ms)", event.pid, mode, grace); }
                                      kill_pid(event.pid, mode, grace).await;
                                 }
                             }
                        }

                        // LOGGING
                        if !args.quiet {
                             // Import from assay-common required?
                             // We use literal values or import
                             match event.event_type {
                                EVENT_OPENAT => println!("[PID {}] openat: {}", event.pid, decode_utf8_cstr(&event.data)),
                                EVENT_CONNECT => println!("[PID {}] connect sockaddr[0..32]=0x{}", event.pid, dump_prefix_hex(&event.data, 32)),
                                10 /* EVENT_FILE_BLOCKED */ => println!("[PID {}] 🛡️ BLOCKED FILE: {}", event.pid, decode_utf8_cstr(&event.data)),
                                11 /* EVENT_FILE_ALLOWED */ => println!("[PID {}] 🟢 ALLOWED FILE: {}", event.pid, decode_utf8_cstr(&event.data)),
                                20 /* EVENT_CONNECT_BLOCKED */ => println!("[PID {}] 🛡️ BLOCKED NET : {}", event.pid, dump_prefix_hex(&event.data, 20)), // IP/Port packed
                                100 => {
                                     // Debug Inode Event
                                     let dev_bytes: [u8; 8] = event.data[0..8].try_into().unwrap_or([0; 8]);
                                     let ino_bytes: [u8; 8] = event.data[8..16].try_into().unwrap_or([0; 8]);
                                     let dev = u64::from_ne_bytes(dev_bytes);
                                     let ino = u64::from_ne_bytes(ino_bytes);
                                     println!("[PID {}] DEBUG: Kernel Saw dev={} ino={}", event.pid, dev, ino);
                                }
                                101 | 102 | 103 | 104 => {
                                    let chunk_idx = event.event_type - 101;
                                    let start_offset = chunk_idx * 64;
                                    let dump = dump_prefix_hex(&event.data, 64);
                                    println!("[PID {}] 🔍 STRUCT DUMP Part {} (Offset {}-{}): {}", event.pid, chunk_idx+1, start_offset, start_offset+64, dump);
                                }
                                105 => {
                                    let path = decode_utf8_cstr(&event.data);
                                    println!("[PID {}] 📂 FILE OPEN (Manual Resolution): {}", event.pid, path);
                                }
                                106 => {
                                    println!("[PID {}] 🐛 DEBUG: Dentry Pointer NULL", event.pid);
                                }
                                107 => {
                                    println!("[PID {}] 🐛 DEBUG: Name Pointer NULL", event.pid);
                                }
                                108 => {
                                    println!("[PID {}] 🐛 DEBUG: LSM Hook Entry (MonitorAll={})", event.pid, event.data[0]);
                                }
                                99 => {
                                     // Debug Cgroup Mismatch
                                     let cg_bytes: [u8; 8] = event.data[0..8].try_into().unwrap_or([0; 8]);
                                     let cgroup_id = u64::from_ne_bytes(cg_bytes);
                                     let path_part = decode_utf8_cstr(&event.data[8..]);
                                     println!("[PID {}] 🐛 LSM DEBUG: Cgroup={} Path={}", event.pid, cgroup_id, path_part);
                                }
                                _ => println!("[PID {}] event {} len={}", event.pid, event.event_type, event.data.len()),
                            }
                        }
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


#[cfg(target_os = "linux")]
use futures::FutureExt; // for .boxed()

#[cfg(target_os="linux")]
fn resolve_cgroup_id(pid: u32) -> anyhow::Result<u64> {
    use std::io::BufRead;
    use std::os::linux::fs::MetadataExt; // for st_ino

    let cgroup_path = format!("/proc/{}/cgroup", pid);
    let file = std::fs::File::open(&cgroup_path)?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        // V2 format: "0::/user.slice/..."
        // Hybrid might have "1:name=systemd:..."
        // We look for "0::" (V2 unified)
        if line.starts_with("0::") {
            let path = line.trim_start_matches("0::");
            let path = if path.is_empty() { "/" } else { path }; // Root cgroup

            let full_path = format!("/sys/fs/cgroup{}", path);
            let metadata = std::fs::metadata(&full_path).map_err(|e| anyhow::anyhow!("Failed to stat {}: {}", full_path, e))?;
            return Ok(metadata.st_ino());
        }
    }

    // Fallback: If no V2 entry, maybe use /proc/self/cgroup inode?
    // Or just fail.
    Err(anyhow::anyhow!("No Cgroup V2 entry found in {}", cgroup_path))
}
