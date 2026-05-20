use assay_runner_spike::RunSpec;
use clap::{Args, Subcommand};
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Clone, Args)]
pub struct RunnerSpikeArgs {
    #[command(subcommand)]
    pub cmd: RunnerSpikeCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RunnerSpikeCommand {
    /// Run a command under the Phase 1 runner-spike contract boundary.
    Run(RunnerSpikeRunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RunnerSpikeRunArgs {
    /// Agent runtime shim to declare for this run.
    #[arg(long, default_value = "none")]
    pub agent_shim: String,

    /// Explicit run id. Defaults to a generated stream-safe id.
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output bundle path. Defaults to assay-runner-spike-<run_id>.tar.gz.
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Hidden S3 spike path: capture live kernel events with assay-monitor.
    #[arg(long, hide = true)]
    pub kernel_capture: bool,

    /// eBPF object path for hidden kernel capture mode.
    #[arg(long, hide = true)]
    pub ebpf: Option<PathBuf>,

    /// Milliseconds to drain kernel events after the child exits.
    #[arg(long, hide = true, default_value_t = 100)]
    pub kernel_drain_ms: u64,

    /// Command to run.
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

pub async fn run(args: RunnerSpikeArgs) -> anyhow::Result<i32> {
    match args.cmd {
        RunnerSpikeCommand::Run(args) => cmd_run(args).await,
    }
}

async fn cmd_run(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    if args.kernel_capture {
        return cmd_run_with_kernel_capture(args).await;
    }

    cmd_run_contract_only(args)
}

fn build_spec(args: &RunnerSpikeRunArgs) -> RunSpec {
    let mut spec = RunSpec::new(args.command.clone()).with_agent_shim(args.agent_shim.clone());
    if let Some(run_id) = &args.run_id {
        spec = spec.with_run_id(run_id.clone());
    }
    spec
}

fn bundle_output_path(args: &RunnerSpikeRunArgs, run_id: &str) -> PathBuf {
    args.output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("assay-runner-spike-{run_id}.tar.gz")))
}

fn cmd_run_contract_only(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    let spec = build_spec(&args);
    let output = bundle_output_path(&args, &spec.run_id);

    let outcome = spec.run_contract_only()?;
    let mut file = File::create(&output)?;
    outcome.archive.write(&mut file)?;
    let exit_status = exit_status_label(outcome.exit_code, outcome.signal);

    println!(
        "wrote runner-spike bundle: {} (run_id={}, status={})",
        output.display(),
        spec.run_id,
        exit_status
    );

    Ok(exit_status_code(outcome.exit_code, outcome.signal))
}

#[cfg(not(target_os = "linux"))]
async fn cmd_run_with_kernel_capture(_args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    eprintln!("Error: runner-spike --kernel-capture is only supported on Linux.");
    Ok(40)
}

#[cfg(target_os = "linux")]
async fn cmd_run_with_kernel_capture(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    use assay_monitor::Monitor;
    use assay_runner_spike::{CgroupCorrelationStatus, KernelLayerBuilder};
    use std::time::{Duration, Instant};
    use tokio_stream::StreamExt;

    let spec = build_spec(&args);
    spec.validate()?;
    let output = bundle_output_path(&args, &spec.run_id);
    let ebpf_path = args
        .ebpf
        .clone()
        .unwrap_or_else(|| PathBuf::from("target/assay-ebpf.o"));

    if !ebpf_path.exists() {
        eprintln!(
            "Error: eBPF object not found at {}. Build it with 'cargo xtask build-ebpf' or provide --ebpf <path>.",
            ebpf_path.display()
        );
        return Ok(40);
    }

    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("Failed to load eBPF: {error}");
            return Ok(40);
        }
    };
    if let Err(error) = monitor.configure_defaults() {
        eprintln!("Failed to configure eBPF defaults: {error}");
        return Ok(40);
    }
    if let Err(error) = monitor.attach() {
        eprintln!("Failed to attach eBPF probes: {error}");
        return Ok(40);
    }

    let before_stats = monitor.snapshot_stats()?;
    // Safe before arming: assay-ebpf default-denies events when
    // MONITORED_CGROUPS is empty and KEY_MONITOR_ALL is unset, so this window
    // loses early child events rather than contaminating the bundle.
    let mut stream = monitor.listen()?;
    let mut builder = KernelLayerBuilder::new(&spec.run_id)?;
    let mut archive = spec.skeleton_archive()?;
    let clock = Instant::now();
    spec.append_run_started(&mut archive, 0, Duration::ZERO)?;

    let mut child = tokio::process::Command::new(&spec.command[0])
        .args(&spec.command[1..])
        .spawn()?;
    if let Some(pid) = child.id() {
        arm_monitor_for_pid(&mut monitor, pid);
    } else {
        eprintln!("Warning: child pid unavailable; kernel capture will be attribution-partial.");
    }

    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            event = stream.next() => {
                match event {
                    Some(Ok(event)) => builder.push_monitor_event(&event)?,
                    Some(Err(error)) => eprintln!("Warning: failed to parse kernel event: {error}"),
                    None => {
                        eprintln!("Warning: kernel event stream closed before child exit.");
                        break child.wait().await?;
                    }
                }
            }
        }
    };

    drain_kernel_events(
        &mut stream,
        &mut builder,
        Duration::from_millis(args.kernel_drain_ms),
    )
    .await?;
    let after_stats = monitor.snapshot_stats()?;
    let capture = builder.finish(&before_stats, &after_stats);
    capture.apply_to_archive(&mut archive, CgroupCorrelationStatus::Partial)?;
    spec.append_run_finished(&mut archive, 1, &status, clock.elapsed())?;

    let mut file = File::create(&output)?;
    archive.write(&mut file)?;
    let exit_code = status.code();
    let signal = exit_signal(&status);
    let exit_status = exit_status_label(exit_code, signal);

    println!(
        "wrote runner-spike bundle: {} (run_id={}, status={}, kernel_capture=partial)",
        output.display(),
        spec.run_id,
        exit_status
    );

    Ok(exit_status_code(exit_code, signal))
}

#[cfg(target_os = "linux")]
fn arm_monitor_for_pid(monitor: &mut assay_monitor::Monitor, pid: u32) {
    if let Err(error) = monitor.set_monitored_pids(&[pid]) {
        eprintln!("Warning: failed to populate runner PID map for {pid}: {error}");
    }

    match resolve_cgroup_id(pid) {
        Ok(cgroup_id) => {
            if let Err(error) = monitor.set_monitored_cgroups(&[cgroup_id]) {
                eprintln!("Warning: failed to populate runner cgroup map for {pid}: {error}");
            }
        }
        Err(error) => {
            eprintln!("Warning: failed to resolve runner cgroup for {pid}: {error}");
        }
    }
}

#[cfg(target_os = "linux")]
async fn drain_kernel_events(
    stream: &mut assay_monitor::EventStream,
    builder: &mut assay_runner_spike::KernelLayerBuilder,
    duration: std::time::Duration,
) -> anyhow::Result<()> {
    use tokio_stream::StreamExt;

    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = stream.next() => {
                match event {
                    Some(Ok(event)) => builder.push_monitor_event(&event)?,
                    Some(Err(error)) => eprintln!("Warning: failed to parse kernel event while draining: {error}"),
                    None => break,
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn resolve_cgroup_id(pid: u32) -> anyhow::Result<u64> {
    use std::io::BufRead;
    use std::os::linux::fs::MetadataExt;

    let cgroup_path = format!("/proc/{pid}/cgroup");
    let file = std::fs::File::open(&cgroup_path)?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("0::") {
            let path = line.trim_start_matches("0::");
            let path = if path.is_empty() { "/" } else { path };
            let full_path = format!("/sys/fs/cgroup{path}");
            let metadata = std::fs::metadata(&full_path)
                .map_err(|error| anyhow::anyhow!("failed to stat {full_path}: {error}"))?;
            return Ok(metadata.st_ino());
        }
    }

    anyhow::bail!("no cgroup v2 entry found in {cgroup_path}")
}

#[cfg(target_os = "linux")]
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

fn exit_status_label(exit_code: Option<i32>, signal: Option<i32>) -> String {
    match (exit_code, signal) {
        (Some(code), _) => format!("exit_code:{code}"),
        (None, Some(signal)) => format!("signal:{signal}"),
        (None, None) => "unknown".to_string(),
    }
}

fn exit_status_code(exit_code: Option<i32>, signal: Option<i32>) -> i32 {
    match (exit_code, signal) {
        (Some(code), _) => code,
        (None, Some(signal)) => 128 + signal,
        (None, None) => 1,
    }
}
