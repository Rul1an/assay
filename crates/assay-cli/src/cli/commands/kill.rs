use clap::Args;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Args, Debug)]
pub struct KillArgs {
    /// Targets: proc-12345, 12345, server_id, or name
    pub targets: Vec<String>,

    /// Kill all running process-based servers (requires --yes)
    #[arg(long)]
    pub all: bool,

    /// Confirm killing all servers
    #[arg(long)]
    pub yes: bool,

    /// Path to inventory file (default: .assay/inventory.yaml or .json)
    #[arg(long, default_value = ".assay/inventory.yaml")]
    pub inventory: PathBuf,

    /// Kill mode: immediate (SIGKILL) or graceful (SIGTERM -> SIGKILL)
    #[arg(long, default_value = "immediate")]
    pub mode: String,

    /// Grace period for graceful mode (e.g., "5s")
    #[arg(long, default_value = "5s")]
    pub grace: String,

    /// Also kill child processes of the target
    #[arg(long)]
    pub kill_children: bool,

    /// Capture state/incident bundle before killing
    #[arg(long)]
    pub capture_state: bool,

    /// Output directory for incident bundle
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Reason for killing (logging purposes)
    #[arg(long)]
    pub reason: Option<String>,
}

pub async fn run(args: KillArgs) -> anyhow::Result<i32> {
    use assay_core::kill_switch::{parse_target_to_pid, KillMode, KillRequest};

    if args.all && !args.yes {
        anyhow::bail!("Refusing to --all without --yes (safety).");
    }

    let mut pids: Vec<u32> = vec![];

    if args.all {
        // Load inventory and collect process PIDs
        let inv = load_inventory(&args.inventory)?;
        // We use the helper we just added
        pids = inv.running_pids();
        if pids.is_empty() {
            println!("No running process-based servers found in inventory.");
            return Ok(0);
        }
        println!("Found {} running servers to kill.", pids.len());
    } else {
        if args.targets.is_empty() {
            anyhow::bail!("Provide at least one target, or use --all.");
        }

        // We only load inventory if we need to resolve names/IDs (i.e. if parsing fails)
        let mut inventory_loaded = None;

        for t in &args.targets {
            if let Some(pid) = parse_target_to_pid(t) {
                pids.push(pid);
                continue;
            }
            // else resolve via inventory by id/name -> pid
            if inventory_loaded.is_none() {
                // Try to load inventory, but don't fail hard if it doesn't exist?
                // The spec implies we should load it.
                // If file missing and user supplied ID, we probably error out or warn.
                match load_inventory(&args.inventory) {
                    Ok(inv) => inventory_loaded = Some(inv),
                    Err(e) => {
                        eprintln!(
                            "Warning: could not load inventory to resolve target '{}': {}",
                            t, e
                        );
                        continue;
                    }
                }
            }

            if let Some(inv) = &inventory_loaded {
                if let Some(pid) = inv.resolve_to_pid(t) {
                    pids.push(pid);
                } else {
                    eprintln!("✗ Could not resolve target '{t}' to a PID (not running process?)");
                }
            }
        }
    }

    let mode = match args.mode.as_str() {
        "immediate" => KillMode::Immediate,
        "graceful" => KillMode::Graceful {
            grace: parse_duration(&args.grace)?,
        },
        other => anyhow::bail!("unknown --mode: {other}"),
    };

    let mut failed = false;

    for pid in pids {
        let req = KillRequest {
            pid,
            mode: mode.clone(),
            kill_children: args.kill_children,
            capture_state: args.capture_state,
            output_dir: args.output.clone(),
            reason: args.reason.clone(),
        };

        match assay_core::kill_switch::kill_pid(req) {
            Ok(rep) if rep.success => {
                println!("✓ killed pid={}", rep.pid);
                if !rep.children_killed.is_empty() {
                    println!("  children killed: {}", rep.children_killed.len());
                }
                if let Some(dir) = rep.incident_dir {
                    println!("  incident: {}", dir.display());
                }
            }
            Ok(rep) => {
                failed = true;
                eprintln!(
                    "✗ failed to kill pid={} ({})",
                    rep.pid,
                    rep.error.unwrap_or_default()
                );
            }
            Err(e) => {
                failed = true;
                eprintln!("✗ failed to kill pid={pid}: {e:#}");
            }
        }
    }

    Ok(if failed { 30 } else { 0 })
}

fn load_inventory(path: &PathBuf) -> anyhow::Result<assay_core::discovery::types::Inventory> {
    // try to read file
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("Failed to read inventory at {:?}: {}", path, e))?;

    // simple heuristic: if extension is json, parse json. else yaml.
    // Spec said default is .assay/inventory.yaml
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        Ok(serde_json::from_slice(&bytes)?)
    } else {
        Ok(serde_yaml::from_slice(&bytes)?)
    }
}

fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    // The spec mentioned humantime, but I need to check if I have it.
    // assay-core doesn't have humantime in dependencies IIRC.
    // assay-cli might?
    // If not, I'll add a simple parser or just error for now.
    // Actually, I'll assume standard humantime behavior is desired and I might need to add the dep.
    // For P0, a simple suffix parser is enough.

    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        return Ok(Duration::from_millis(ms.parse()?));
    }
    if let Some(sec) = s.strip_suffix("s") {
        return Ok(Duration::from_secs(sec.parse()?));
    }
    // Fallback: assume seconds
    Ok(Duration::from_secs(s.parse()?))
}
