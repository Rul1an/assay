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
    use crate::cgroup::CgroupManager;
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

    let cgroup_manager = match CgroupManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("Failed to initialize runner cgroup manager: {error}");
            return Ok(40);
        }
    };
    let session_cgroup = match cgroup_manager.create_session() {
        Ok(cgroup) => cgroup,
        Err(error) => {
            eprintln!("Failed to create runner cgroup session: {error}");
            return Ok(40);
        }
    };
    if let Err(error) = monitor.set_monitored_cgroups(&[session_cgroup.id()]) {
        eprintln!("Failed to populate runner cgroup map: {error}");
        return Ok(40);
    }

    let before_stats = monitor.snapshot_stats()?;
    // Stream is armed against the empty session cgroup. No events flow until
    // pre_exec moves the child into that cgroup below, which avoids the
    // listen-before-arm loss window from the partial capture path.
    let mut stream = monitor.listen()?;
    let mut builder = KernelLayerBuilder::new(&spec.run_id)?;
    let mut archive = spec.skeleton_archive()?;
    let clock = Instant::now();
    spec.append_run_started(&mut archive, 0, Duration::ZERO)?;

    let mut child = spawn_child_in_cgroup(&spec, &session_cgroup)?;

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
    capture.apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)?;
    spec.append_run_finished(&mut archive, 1, &status, clock.elapsed())?;

    let mut file = File::create(&output)?;
    archive.write(&mut file)?;
    let exit_code = status.code();
    let signal = exit_signal(&status);
    let exit_status = exit_status_label(exit_code, signal);

    println!(
        "wrote runner-spike bundle: {} (run_id={}, status={}, kernel_capture=clean)",
        output.display(),
        spec.run_id,
        exit_status
    );

    Ok(exit_status_code(exit_code, signal))
}

#[cfg(target_os = "linux")]
fn spawn_child_in_cgroup(
    spec: &RunSpec,
    cgroup: &crate::cgroup::SessionCgroup,
) -> anyhow::Result<tokio::process::Child> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let procs_path = CString::new(cgroup.procs_path().as_os_str().as_bytes())?;
    let mut command = tokio::process::Command::new(&spec.command[0]);
    command.args(&spec.command[1..]);

    unsafe {
        command.pre_exec(move || write_self_to_cgroup(&procs_path));
    }

    command
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to spawn child in runner cgroup: {error}"))
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
fn write_self_to_cgroup(procs_path: &std::ffi::CStr) -> std::io::Result<()> {
    let fd = unsafe { libc::open(procs_path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let pid = unsafe { libc::getpid() } as u32;
    let mut buf = [0_u8; 32];
    let len = write_u32_decimal(pid, &mut buf);
    let written = unsafe { libc::write(fd, buf.as_ptr().cast(), len) };
    let write_error = if written < 0 || written as usize != len {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    let close_result = unsafe { libc::close(fd) };

    match (write_error, close_result) {
        (Some(error), _) => Err(error),
        (None, -1) => Err(std::io::Error::last_os_error()),
        (None, _) => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn write_u32_decimal(value: u32, buf: &mut [u8; 32]) -> usize {
    let mut n = value;
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut scratch = [0_u8; 10];
    let mut len = 0;
    while n > 0 {
        scratch[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for idx in 0..len {
        buf[idx] = scratch[len - idx - 1];
    }
    len
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn write_u32_decimal_writes_pid_bytes_without_allocation() {
        let mut buf = [0_u8; 32];

        let len = write_u32_decimal(12345, &mut buf);

        assert_eq!(&buf[..len], b"12345");
    }
}
