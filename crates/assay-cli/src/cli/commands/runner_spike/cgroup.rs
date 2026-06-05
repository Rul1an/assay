use super::args::RunnerSpikeRunArgs;

#[cfg(not(target_os = "linux"))]
pub(super) async fn cmd_run_with_kernel_capture(_args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    eprintln!("Error: runner-spike --kernel-capture is only supported on Linux.");
    Ok(40)
}

#[cfg(target_os = "linux")]
pub(super) async fn cmd_run_with_kernel_capture(args: RunnerSpikeRunArgs) -> anyhow::Result<i32> {
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use assay_monitor::Monitor;
    use assay_runner_core::KernelLayerBuilder;
    use assay_runner_linux::CgroupManager;
    use assay_runner_schema::CgroupCorrelationStatus;
    use tokio_stream::StreamExt;

    use super::exit_status::{
        cgroup_correlation_label, exit_signal, exit_status_code, exit_status_label,
    };
    use super::logs::apply_policy_then_sdk_logs_if_requested;
    use super::phases::{record_phase, write_phase_timing_log};
    use super::spec::{build_spec, bundle_output_path};

    let total_start = Instant::now();
    let mut phases = BTreeMap::new();
    let phase_log = args.phase_timing_log.clone();
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
        record_phase(&mut phases, "preflight_ms", total_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("ebpf_object_missing"),
        )?;
        return Ok(40);
    }

    record_phase(&mut phases, "preflight_ms", total_start);

    let monitor_start = Instant::now();
    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("Failed to load eBPF: {error}");
            record_phase(&mut phases, "monitor_attach_ms", monitor_start);
            write_phase_timing_log(
                phase_log.as_ref(),
                &spec,
                &phases,
                Some(40),
                None,
                Some("ebpf_load_failed"),
            )?;
            return Ok(40);
        }
    };
    if let Err(error) = monitor.configure_defaults() {
        eprintln!("Failed to configure eBPF defaults: {error}");
        record_phase(&mut phases, "monitor_attach_ms", monitor_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("ebpf_configure_failed"),
        )?;
        return Ok(40);
    }
    if let Err(error) = monitor.set_emit_inode_resolved(false) {
        eprintln!("Failed to disable runner-spike inode telemetry: {error}");
        record_phase(&mut phases, "monitor_attach_ms", monitor_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("ebpf_inode_telemetry_config_failed"),
        )?;
        return Ok(40);
    }
    if let Err(error) = monitor.set_dedup_open_paths(true) {
        eprintln!("Failed to enable runner-spike open path dedupe: {error}");
        record_phase(&mut phases, "monitor_attach_ms", monitor_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("ebpf_open_path_dedupe_config_failed"),
        )?;
        return Ok(40);
    }
    if let Err(error) = monitor.attach() {
        eprintln!("Failed to attach eBPF probes: {error}");
        record_phase(&mut phases, "monitor_attach_ms", monitor_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("ebpf_attach_failed"),
        )?;
        return Ok(40);
    }
    record_phase(&mut phases, "monitor_attach_ms", monitor_start);

    let cgroup_start = Instant::now();
    let cgroup_manager = match CgroupManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("Failed to initialize runner cgroup manager: {error}");
            record_phase(&mut phases, "cgroup_prepare_ms", cgroup_start);
            write_phase_timing_log(
                phase_log.as_ref(),
                &spec,
                &phases,
                Some(40),
                None,
                Some("cgroup_manager_init_failed"),
            )?;
            return Ok(40);
        }
    };
    let session_cgroup = match cgroup_manager.create_session() {
        Ok(cgroup) => cgroup,
        Err(error) => {
            eprintln!("Failed to create runner cgroup session: {error}");
            record_phase(&mut phases, "cgroup_prepare_ms", cgroup_start);
            write_phase_timing_log(
                phase_log.as_ref(),
                &spec,
                &phases,
                Some(40),
                None,
                Some("cgroup_session_create_failed"),
            )?;
            return Ok(40);
        }
    };
    if let Err(error) = monitor.set_monitored_cgroups(&[session_cgroup.id()]) {
        eprintln!("Failed to populate runner cgroup map: {error}");
        record_phase(&mut phases, "cgroup_prepare_ms", cgroup_start);
        write_phase_timing_log(
            phase_log.as_ref(),
            &spec,
            &phases,
            Some(40),
            None,
            Some("cgroup_monitor_map_failed"),
        )?;
        return Ok(40);
    }
    record_phase(&mut phases, "cgroup_prepare_ms", cgroup_start);

    let before_stats = monitor.snapshot_stats()?;
    // Stream is armed against the empty session cgroup. No events flow until
    // pre_exec moves the child into that cgroup below, which avoids the
    // listen-before-arm loss window from the partial capture path.
    let mut stream = monitor.listen()?;
    let mut builder = KernelLayerBuilder::new(&spec.run_id)?;
    let mut archive = spec.skeleton_archive()?;
    let clock = Instant::now();
    spec.append_run_started(&mut archive, 0, Duration::ZERO)?;

    let child_spawn_start = Instant::now();
    let mut child = match spawn_child_in_cgroup(&spec, &session_cgroup) {
        Ok(child) => {
            record_phase(&mut phases, "child_spawn_ms", child_spawn_start);
            child
        }
        Err(error) => {
            record_phase(&mut phases, "child_spawn_ms", child_spawn_start);
            write_phase_timing_log(
                phase_log.as_ref(),
                &spec,
                &phases,
                None,
                None,
                Some("child_spawn_failed"),
            )?;
            return Err(error);
        }
    };
    let mut cgroup_correlation = CgroupCorrelationStatus::Clean;

    let child_runtime_start = Instant::now();
    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            event = stream.next() => {
                match event {
                    Some(Ok(event)) => builder.push_monitor_event(&event)?,
                    Some(Err(error)) => {
                        eprintln!("Warning: failed to parse kernel event: {error}");
                        cgroup_correlation = CgroupCorrelationStatus::Partial;
                    }
                    None => {
                        eprintln!("Warning: kernel event stream closed before child exit.");
                        cgroup_correlation = CgroupCorrelationStatus::Partial;
                        break child.wait().await?;
                    }
                }
            }
        }
    };
    record_phase(&mut phases, "child_runtime_ms", child_runtime_start);

    let event_flush_start = Instant::now();
    let drain_complete = drain_kernel_events(
        &mut stream,
        &mut builder,
        Duration::from_millis(args.kernel_drain_ms),
    )
    .await?;
    if !drain_complete {
        cgroup_correlation = CgroupCorrelationStatus::Partial;
    }
    // Closing the receiver lets the monitor listener break out of blocking_send
    // before snapshot_stats() tries to lock the shared BPF state again.
    drop(stream);
    let after_stats = monitor.snapshot_stats()?;
    let capture = builder.finish(&before_stats, &after_stats);
    capture.apply_to_archive(&mut archive, cgroup_correlation)?;
    apply_policy_then_sdk_logs_if_requested(&spec, &args, &mut archive)?;
    spec.append_run_finished(&mut archive, 1, &status, clock.elapsed())?;
    record_phase(&mut phases, "event_flush_ms", event_flush_start);

    let archive_write_start = Instant::now();
    let mut file = File::create(&output)?;
    archive.write(&mut file)?;
    record_phase(&mut phases, "archive_write_ms", archive_write_start);
    let exit_code = status.code();
    let signal = exit_signal(&status);
    let exit_status = exit_status_label(exit_code, signal);
    write_phase_timing_log(phase_log.as_ref(), &spec, &phases, exit_code, signal, None)?;

    println!(
        "wrote runner-spike bundle: {} (run_id={}, status={}, kernel_capture={})",
        output.display(),
        spec.run_id,
        exit_status,
        cgroup_correlation_label(cgroup_correlation)
    );

    Ok(exit_status_code(exit_code, signal))
}

#[cfg(target_os = "linux")]
fn spawn_child_in_cgroup(
    spec: &assay_runner_core::RunSpec,
    cgroup: &assay_runner_linux::SessionCgroup,
) -> anyhow::Result<tokio::process::Child> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let procs_path = CString::new(cgroup.procs_path().as_os_str().as_bytes())?;
    let mut command = tokio::process::Command::new(&spec.command[0]);
    command.args(&spec.command[1..]);
    apply_kernel_capture_child_env(&mut command, spec);

    unsafe {
        command.pre_exec(move || write_self_to_cgroup(&procs_path));
    }

    command
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to spawn child in runner cgroup: {error}"))
}

#[cfg(target_os = "linux")]
fn apply_kernel_capture_child_env(
    command: &mut tokio::process::Command,
    spec: &assay_runner_core::RunSpec,
) {
    // `cargo run` injects dynamic-loader search paths into the parent process.
    // If inherited by the fixture, every shell/tool startup emits thousands of
    // loader/locale openat events that are not runner-spike attribution
    // evidence and vary across runs. Keep PATH and caller env intact, but
    // remove loader hooks and pin locale behavior before applying spec env.
    for key in [
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "LOCPATH",
        "GCONV_PATH",
    ] {
        command.env_remove(key);
    }
    command.env("LC_ALL", "C");
    command.env("LANG", "C");
    command.envs(&spec.env);
}

#[cfg(target_os = "linux")]
async fn drain_kernel_events(
    stream: &mut assay_monitor::EventStream,
    builder: &mut assay_runner_core::KernelLayerBuilder,
    duration: std::time::Duration,
) -> anyhow::Result<bool> {
    use tokio_stream::StreamExt;

    let mut complete = true;
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = stream.next() => {
                match event {
                    Some(Ok(event)) => builder.push_monitor_event(&event)?,
                    Some(Err(error)) => {
                        eprintln!("Warning: failed to parse kernel event while draining: {error}");
                        complete = false;
                    }
                    None => {
                        complete = false;
                        break;
                    }
                }
            }
        }
    }
    Ok(complete)
}

#[cfg(target_os = "linux")]
fn write_self_to_cgroup(procs_path: &std::ffi::CStr) -> std::io::Result<()> {
    let fd = retry_open_write_only(procs_path)?;

    let pid = unsafe { libc::getpid() } as u32;
    let mut buf = [0_u8; 32];
    let len = write_u32_decimal(pid, &mut buf);
    let write_result = retry_write_all(fd, &buf[..len]);
    let close_result = unsafe { libc::close(fd) };

    match (write_result.err(), close_result) {
        (Some(error), _) => Err(error),
        (None, -1) => Err(std::io::Error::last_os_error()),
        (None, _) => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn retry_open_write_only(path: &std::ffi::CStr) -> std::io::Result<i32> {
    loop {
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC) };
        if fd >= 0 {
            return Ok(fd);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(error);
        }
    }
}

#[cfg(target_os = "linux")]
fn retry_write_all(fd: i32, mut bytes: &[u8]) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(error);
        }
        if written == 0 {
            return Err(std::io::Error::from_raw_os_error(libc::EIO));
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
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
