#[allow(unused_imports)]
use crate::cli::commands::exit_codes;
use clap::Args;
use std::path::PathBuf;

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
    use assay_monitor::{Monitor, MonitorError};
    use assay_common::{EVENT_OPENAT, EVENT_CONNECT};

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

    // 5. Stream
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
                        // Logic: if quiet -> skip.
                        // else if print OR it's enabled by default (?) -> print.
                        // User said: "anders als print OR “geen outputs gespecificeerd” => print"
                        // But since print default is now FALSE, we must assume default behavior implies printing?
                        // "implied if no other output specified" -> usually implies print is TRUE by default if nothing else.
                        // But user set default to false.
                        // Let's assume: if !quiet => print.
                        if !args.quiet {
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
