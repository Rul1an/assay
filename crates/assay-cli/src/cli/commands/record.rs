//! Assay Record: Unified Capture + Generate Flow

use crate::cli::commands::generate;
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Args, Debug, Clone)]
pub struct RecordArgs {
    /// Command to record
    #[arg(last = true, required = true)]
    pub command: Vec<String>,

    /// Policy file to write
    #[arg(short, long, default_value = "policy.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value = "Recorded Policy")]
    pub name: String,

    /// Duration to wait before stopping monitor after command exits (seconds)
    #[arg(long, default_value_t = 1)]
    pub settle_duration: u64,
}

pub async fn run(args: RecordArgs) -> Result<i32> {
    let trace_file = std::env::temp_dir().join(format!("assay-trace-{}.jsonl", std::process::id()));

    eprintln!(">>> Starting background monitor...");
    eprintln!(">>> Output trace: {}", trace_file.display());

    // 1. Start Monitor in background
    // We use the same binary "assay" but with "monitor" subcommand
    let my_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("assay"));

    // Ensure trace file is clean
    if trace_file.exists() {
        std::fs::remove_file(&trace_file)?;
    }

    struct TraceGuard(PathBuf);
    impl Drop for TraceGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _guard = TraceGuard(trace_file.clone());

    let mut monitor = Command::new(&my_exe)
        .arg("monitor")
        .arg("--output")
        .arg(&trace_file)
        // We assume 'assay monitor' has a flag or behavior to handle non-interactive bg run
        // But 'monitor' runs until signal. We will kill it.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn assay monitor")?;

    // Give it a moment to attach
    // Wait for monitor to be ready by polling for the trace file to appear,
    // with a conservative timeout to avoid hanging indefinitely.
    let monitor_attach_timeout = Duration::from_secs(5);
    let poll_interval = Duration::from_millis(100);
    let attach_start = std::time::Instant::now();
    loop {
        if trace_file.exists() {
            break;
        }
        if attach_start.elapsed() >= monitor_attach_timeout {
            eprintln!(
                ">>> Monitor did not signal readiness within {:?}; proceeding anyway.",
                monitor_attach_timeout
            );
            break;
        }
        sleep(poll_interval).await;
    }

    // 2. Run User Command
    let (cmd, cmd_args) = args
        .command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("no command provided to run"))?;

    eprintln!(">>> Running: {:?}", args.command);

    // Check if monitor is still alive/running before we do anything
    if let Ok(Some(status)) = monitor.try_wait() {
        return Err(anyhow::anyhow!(
            "Monitor process died early with status: {}",
            status
        ));
    }

    let status = Command::new(cmd)
        .args(cmd_args)
        .status()
        .context("Failed to run user command")?;

    eprintln!(">>> Command finished with status: {}", status);

    // 3. Settle
    if args.settle_duration > 0 {
        eprintln!(
            ">>> Waiting {}s for events to settle...",
            args.settle_duration
        );
        sleep(Duration::from_secs(args.settle_duration)).await;
    }

    // 4. Stop Monitor
    eprintln!(">>> Stopping monitor...");
    let _ = monitor.kill(); // SIGKILL/SIGTERM equivalent
    let _ = monitor.wait(); // Reap zombie

    // 5. Generate Policy
    eprintln!(">>> Generating policy to {}...", args.output.display());

    let gen_args = generate::GenerateArgs {
        input: Some(trace_file.clone()),
        profile: None,
        output: args.output.clone(),
        name: args.name,
        format: "yaml".to_string(),
        dry_run: false,
        diff: false,
        heuristics: true, // Enable heuristics for record mode
        entropy_threshold: 3.8,
        min_stability: 0.8,
        review_threshold: 0.6,
        new_is_risky: false,
        alpha: 1.0,
        min_runs: 0,    // single-run mode
        wilson_z: 1.96, // SOTA: 95% confidence gating
    };

    // We can call generate::run directly since it's in-process
    // Note: generate::run is synchronous currently, but that's fine inside async fn
    generate::run(gen_args)
}
