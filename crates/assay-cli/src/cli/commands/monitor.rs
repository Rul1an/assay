
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
}


pub async fn run(args: MonitorArgs) -> anyhow::Result<i32> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        eprintln!("Error: 'assay monitor' is only supported on Linux.");
        return Ok(40); // MONITOR_NOT_SUPPORTED code
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

    // 0. Load Policy (Optional)
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

    // 1. Resolve eBPF path
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

    // 2. Load
    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to load eBPF: {}", e);
            return Ok(40); // MONITOR_ATTACH_FAILED
        }
    };

    // 3. Configure
    if !args.pid.is_empty() {
        if let Err(e) = monitor.set_monitored_pids(&args.pid) {
            eprintln!("Failed to set monitored PIDs: {}", e);
            return Ok(40);
        }
    }

    // 4. Attach
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

    // 5. Build Matchers
    #[cfg(target_os = "linux")]
    #[derive(Debug)]
    struct ActiveRule {
        id: String,
        action: assay_core::mcp::runtime_features::MonitorAction,

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
        let mut parts = Vec::new();
        for part in input.split('/') {
            match part {
                "" | "." => {}
                ".." => { parts.pop(); }
                x => parts.push(x),
            }
        }
        let mut out = String::from("/");
        out.push_str(&parts.join("/"));
        out
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

    // 6. Stream and Enforce (Wait, we need monitor!)
    // The previous snippet reused 'monitor' which was moved/consumed by 'monitor.listen()'.
    // 'monitor.listen()' consumes 'self'.
    // So 'monitor' is gone. We used 'stream' from Step 5.

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

                        // ENFORCEMENT LOGIC (Linux Only)
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

                        // REACT
                        if let Some(rule) = violation_rule {
                             if args.print && !args.quiet {
                                 println!("[PID {}] 🚨 VIOLATION: Rule '{}' matched file access", event.pid, rule.id);
                             }

                             if rule.action == assay_core::mcp::runtime_features::MonitorAction::TriggerKill {
                                 // Check Kill Switch Configuration
                                 // Default to SAFE defaults if config missing (though it shouldn't be null if policy loaded)
                                 // But 'kill_config' is Option<KillSwitchConfig>.

                                 // Logic:
                                 // 1. Is Kill Switch enabled globally?
                                 // 2. Is there a specific trigger override for this rule? (Not implemented in snippet, user asked for Phase 4 compliance)
                                 // User said: "respecteer kill_switch.enabled, mode + eventuele override".

                                 // Default fallback
                                 let default_mode = assay_core::mcp::runtime_features::KillMode::Graceful;
                                 let default_grace = 3000;

                                 let (enabled, mode, grace) = if let Some(kc) = &kill_config {
                                     // Check for specific trigger override
                                     let trigger = kc.triggers.iter().find(|t| t.on_rule == rule.id);
                                     let mode = trigger.and_then(|t| t.mode.clone()).unwrap_or(kc.mode.clone());
                                     (kc.enabled, mode, kc.grace_period_ms)
                                 } else {
                                     (false, default_mode, default_grace)
                                 };

                                 if enabled {
                                      if args.print && !args.quiet { println!("[PID {}] 💀 INIT KILL (mode={:?}, grace={}ms)", event.pid, mode, grace); }
                                      kill_pid(event.pid, mode, grace).await;
                                 }
                             }
                        }

                        // LOGGING
                        if args.print && !args.quiet {
                             match event.event_type {
                                EVENT_OPENAT => println!("[PID {}] openat: {}", event.pid, decode_utf8_cstr(&event.data)),
                                EVENT_CONNECT => println!("[PID {}] connect sockaddr[0..32]=0x{}", event.pid, dump_prefix_hex(&event.data, 32)),
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
