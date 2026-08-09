use std::ffi::OsStr;
use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const REAP_GRACE: Duration = Duration::from_secs(1);
const MAX_DIAGNOSTIC_BYTES: usize = 4096;

#[derive(Clone, Copy)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

impl ProcessLimits {
    pub const fn new(timeout: Duration, max_stdout_bytes: usize, max_stderr_bytes: usize) -> Self {
        Self {
            timeout,
            max_stdout_bytes,
            max_stderr_bytes,
        }
    }
}

pub const GOLDEN_PATH_LIMITS: ProcessLimits =
    ProcessLimits::new(Duration::from_secs(10), 1024 * 1024, 1024 * 1024);

#[derive(Clone, Copy)]
enum CapturedStream {
    Stdout,
    Stderr,
}

impl CapturedStream {
    fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// Runs one non-interactive command and owns its subprocess tree for the call.
///
/// A hard termination is sent to descendants that remain in the Unix process
/// group or Windows Job Object before this function returns. Processes that
/// deliberately escape those OS containers are outside this test helper's
/// supported process shape.
pub fn run_bounded(
    mut command: Command,
    stdin: &[u8],
    limits: ProcessLimits,
    context: &str,
) -> Result<Output, String> {
    assert!(
        limits.max_stdout_bytes > 0,
        "stdout ceiling must be positive"
    );
    assert!(
        limits.max_stderr_bytes > 0,
        "stderr ceiling must be positive"
    );
    let command_display = display_command(&command);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = CommandWrap::from(command);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(JobObject);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{context}: spawn {command_display}: {error}"))?;
    let mut child_stdin = child
        .stdin()
        .take()
        .ok_or_else(|| format!("{context}: {command_display}: child stdin was not piped"))?;
    let child_stdout = child
        .stdout()
        .take()
        .ok_or_else(|| format!("{context}: {command_display}: child stdout was not piped"))?;
    let child_stderr = child
        .stderr()
        .take()
        .ok_or_else(|| format!("{context}: {command_display}: child stderr was not piped"))?;

    let stdin = stdin.to_vec();
    let stdin_writer = thread::spawn(move || {
        let result = child_stdin.write_all(&stdin);
        drop(child_stdin);
        result
    });

    let (overflow_tx, overflow_rx) = mpsc::channel();
    let stdout_reader = spawn_reader(
        child_stdout,
        limits.max_stdout_bytes,
        CapturedStream::Stdout,
        overflow_tx.clone(),
    );
    let stderr_reader = spawn_reader(
        child_stderr,
        limits.max_stderr_bytes,
        CapturedStream::Stderr,
        overflow_tx,
    );

    let deadline = Instant::now() + limits.timeout;
    let mut early_failure = None;
    let mut observed_status = None;
    let status = loop {
        if early_failure.is_none() {
            if let Ok(stream) = overflow_rx.try_recv() {
                // The reader closes its pipe after limit + 1 bytes. Keep the
                // child under this same deadline while it observes the close.
                early_failure = Some(format!(
                    "{} exceeded its {}-byte ceiling",
                    stream.name(),
                    stream_limit(limits, stream)
                ));
            }
        }

        if observed_status.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => observed_status = Some(status),
                Ok(None) => {}
                Err(error) => {
                    let reap = terminate_and_reap(child.as_mut(), None, context, &command_display);
                    return Err(format!(
                        "{context}: poll {command_display}: {error}; terminate/reap: {reap:?}"
                    ));
                }
            }
        }

        if observed_status.is_some()
            && stdin_writer.is_finished()
            && stdout_reader.is_finished()
            && stderr_reader.is_finished()
        {
            finish_process_tree(child.as_mut(), context, &command_display)?;
            break observed_status
                .take()
                .expect("status was checked immediately above");
        }

        if Instant::now() >= deadline {
            if early_failure.is_none() {
                early_failure = Some(format!("deadline of {:?} expired", limits.timeout));
            }
            break terminate_and_reap(
                child.as_mut(),
                observed_status.take(),
                context,
                &command_display,
            )?;
        }
        thread::sleep(POLL_INTERVAL);
    };

    let stdin_result = stdin_writer
        .join()
        .map_err(|_| format!("{context}: {command_display}: stdin writer panicked"))?;
    let stdout = join_reader(stdout_reader, context, &command_display, "stdout")?;
    let stderr = join_reader(stderr_reader, context, &command_display, "stderr")?;

    if let Some(failure) = early_failure {
        return Err(failure_diagnostic(
            context,
            &command_display,
            &failure,
            &status,
            &stdout,
            &stderr,
        ));
    }
    if stdout.len() > limits.max_stdout_bytes {
        return Err(failure_diagnostic(
            context,
            &command_display,
            &format!(
                "stdout exceeded its {}-byte ceiling",
                limits.max_stdout_bytes
            ),
            &status,
            &stdout,
            &stderr,
        ));
    }
    if stderr.len() > limits.max_stderr_bytes {
        return Err(failure_diagnostic(
            context,
            &command_display,
            &format!(
                "stderr exceeded its {}-byte ceiling",
                limits.max_stderr_bytes
            ),
            &status,
            &stdout,
            &stderr,
        ));
    }
    // Status and both output streams are already collected, so BrokenPipe here
    // records an early stdin close without replacing the child's outcome.
    stdin_result.or_else(|error| {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        Err(failure_diagnostic(
            context,
            &command_display,
            &format!("write stdin: {error}"),
            &status,
            &stdout,
            &stderr,
        ))
    })?;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    limit: usize,
    stream: CapturedStream,
    overflow: mpsc::Sender<CapturedStream>,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::with_capacity(limit.saturating_add(1));
        reader
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            let _ = overflow.send(stream);
        }
        Ok(bytes)
    })
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    context: &str,
    command: &str,
    stream: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("{context}: {command}: {stream} reader panicked"))?
        .map_err(|error| format!("{context}: read {stream} from {command}: {error}"))
}

fn terminate_and_reap(
    child: &mut dyn ChildWrapper,
    observed_status: Option<ExitStatus>,
    context: &str,
    command: &str,
) -> Result<ExitStatus, String> {
    let tree_error = child
        .start_kill()
        .err()
        .filter(|error| !process_tree_absent(error));
    let direct_kill_error = tree_error
        .as_ref()
        .and_then(|_| child.inner_mut().start_kill().err());
    let status = reap_after_tree_kill(child, observed_status).map_err(|reap_error| {
        format!(
            "{context}: failed to reap {command} after tree termination; tree={tree_error:?}; direct_kill={direct_kill_error:?}; reap={reap_error}"
        )
    })?;
    if let Some(tree_error) = tree_error {
        return Err(format!(
            "{context}: failed to terminate process tree for {command}: {tree_error}; direct_kill={direct_kill_error:?}; status={status}"
        ));
    }
    Ok(status)
}

fn reap_after_tree_kill(
    child: &mut dyn ChildWrapper,
    observed_status: Option<ExitStatus>,
) -> Result<ExitStatus, String> {
    if let Some(status) = observed_status {
        return Ok(status);
    }

    let deadline = Instant::now() + REAP_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            Ok(None) => return Err(format!("reap grace of {REAP_GRACE:?} expired")),
            Err(error) => return Err(format!("poll after termination: {error}")),
        }
    }
}

fn finish_process_tree(
    child: &mut dyn ChildWrapper,
    context: &str,
    command: &str,
) -> Result<(), String> {
    match child.start_kill() {
        Ok(()) => Ok(()),
        Err(error) if process_tree_quiescent_after_io(&error) => Ok(()),
        Err(error) => Err(format!(
            "{context}: terminate remaining process tree for {command}: {error}"
        )),
    }
}

#[cfg(unix)]
fn process_tree_absent(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(windows)]
fn process_tree_absent(_error: &std::io::Error) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn process_tree_quiescent_after_io(error: &std::io::Error) -> bool {
    // macOS may report EPERM for an already-empty group. This is accepted only
    // after child status, stdin, stdout, and stderr are all complete, so it can
    // never lead into a blocking join on a live inherited pipe.
    matches!(error.raw_os_error(), Some(libc::ESRCH) | Some(libc::EPERM))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_tree_quiescent_after_io(error: &std::io::Error) -> bool {
    process_tree_absent(error)
}

#[cfg(windows)]
fn process_tree_quiescent_after_io(error: &std::io::Error) -> bool {
    process_tree_absent(error)
}

fn stream_limit(limits: ProcessLimits, stream: CapturedStream) -> usize {
    match stream {
        CapturedStream::Stdout => limits.max_stdout_bytes,
        CapturedStream::Stderr => limits.max_stderr_bytes,
    }
}

fn display_command(command: &Command) -> String {
    let mut parts = vec![display_os(command.get_program())];
    parts.extend(command.get_args().map(display_os));
    let rendered = parts.join(" ");
    match command.get_current_dir() {
        Some(cwd) => format!("{rendered} (cwd {})", cwd.display()),
        None => rendered,
    }
}

fn display_os(value: &OsStr) -> String {
    format!("{:?}", value.to_string_lossy())
}

fn failure_diagnostic(
    context: &str,
    command: &str,
    failure: &str,
    status: &ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> String {
    format!(
        "{context}: {command}: {failure}; status={status}; stdout={}; stderr={}",
        excerpt(stdout),
        excerpt(stderr)
    )
}

fn excerpt(bytes: &[u8]) -> String {
    let kept = &bytes[..bytes.len().min(MAX_DIAGNOSTIC_BYTES)];
    let suffix = if kept.len() < bytes.len() {
        format!("... ({} bytes total)", bytes.len())
    } else {
        format!(" ({} bytes)", bytes.len())
    };
    format!("{:?}{suffix}", String::from_utf8_lossy(kept))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{process_tree_absent, process_tree_quiescent_after_io};
    use super::{run_bounded, ProcessLimits};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    fn hanging_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "while :; do :; done"]);
        command
    }

    #[cfg(windows)]
    fn hanging_command() -> Command {
        let mut command = Command::new("ping");
        command.args(["-t", "127.0.0.1"]);
        command
    }

    #[cfg(unix)]
    fn stdout_flood_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "while :; do printf '0123456789abcdef'; done"]);
        command
    }

    #[cfg(windows)]
    fn stdout_flood_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "for /L %i in (1,1,1000000) do @echo 0123456789abcdef"]);
        command
    }

    #[cfg(unix)]
    fn stderr_flood_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "while :; do printf '0123456789abcdef' >&2; done"]);
        command
    }

    #[cfg(unix)]
    fn inherited_output_descendant_command(pid_file: &Path) -> Command {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                "sh -c 'echo \"$$\" > \"$1\"; while :; do :; done' descendant \"$1\" & printf parent-ready",
            )
            .arg("runner")
            .arg(pid_file);
        command
    }

    #[cfg(windows)]
    fn inherited_output_descendant_command(pid_file: &Path) -> Command {
        let mut command = Command::new("powershell.exe");
        command.env("ASSAY_DESCENDANT_PID_FILE", pid_file);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$path = [Environment]::GetEnvironmentVariable('ASSAY_DESCENDANT_PID_FILE'); $p = Start-Process -FilePath ping.exe -ArgumentList @('-t','127.0.0.1') -NoNewWindow -PassThru; [IO.File]::WriteAllText($path, [string]$p.Id); [Console]::Out.Write('parent-ready')",
        ]);
        command
    }

    #[cfg(unix)]
    fn early_exit_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 23"]);
        command
    }

    #[cfg(windows)]
    fn early_exit_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit", "/B", "23"]);
        command
    }

    fn read_descendant_pid(pid_file: &Path) -> Option<u32> {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Ok(raw) = std::fs::read_to_string(pid_file) {
                if let Ok(pid) = raw.trim().parse() {
                    return Some(pid);
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[cfg(unix)]
    fn descendant_is_alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    fn descendant_is_alive(pid: u32) -> bool {
        let mut command = Command::new("powershell.exe");
        command
            .env("ASSAY_DESCENDANT_PID", pid.to_string())
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$targetPid = [Environment]::GetEnvironmentVariable('ASSAY_DESCENDANT_PID'); if (Get-Process -Id $targetPid -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }",
            ]);
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn wait_for_descendant_exit(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !descendant_is_alive(pid) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        !descendant_is_alive(pid)
    }

    #[cfg(windows)]
    fn stderr_flood_command() -> Command {
        let mut command = Command::new("cmd");
        command.args([
            "/C",
            "for /L %i in (1,1,1000000) do @echo 0123456789abcdef 1>&2",
        ]);
        command
    }

    #[cfg(unix)]
    fn descendant_run_timeout() -> Duration {
        Duration::from_millis(250)
    }

    #[cfg(windows)]
    fn descendant_run_timeout() -> Duration {
        Duration::from_secs(5)
    }

    #[cfg(unix)]
    fn descendant_test_guard() -> Duration {
        Duration::from_secs(2)
    }

    #[cfg(windows)]
    fn descendant_test_guard() -> Duration {
        Duration::from_secs(10)
    }

    #[cfg(unix)]
    fn quiet_descendant_command(pid_file: &Path) -> Command {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                "sh -c 'echo \"$$\" > \"$1\"; exec </dev/null >/dev/null 2>&1; while :; do :; done' descendant \"$1\" & while [ ! -s \"$1\" ]; do :; done; printf parent-ready",
            )
            .arg("runner")
            .arg(pid_file);
        command
    }

    #[cfg(windows)]
    fn quiet_descendant_command(pid_file: &Path) -> Command {
        let mut command = Command::new("powershell.exe");
        command.env("ASSAY_DESCENDANT_PID_FILE", pid_file);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$path = [Environment]::GetEnvironmentVariable('ASSAY_DESCENDANT_PID_FILE'); $p = Start-Process -FilePath ping.exe -ArgumentList @('-t','127.0.0.1') -WindowStyle Hidden -PassThru; [IO.File]::WriteAllText($path, [string]$p.Id); [Console]::Out.Write('parent-ready')",
        ]);
        command
    }

    #[test]
    fn kills_timeout_and_reports_context() {
        let command = hanging_command();
        let limits = ProcessLimits::new(Duration::from_millis(100), 1024, 1024);
        let error = run_bounded(command, b"", limits, "hanging mutation")
            .expect_err("hanging child must time out");
        assert!(error.contains("hanging mutation"));
        assert!(error.contains("deadline"));
    }

    #[test]
    fn kills_stdout_flood() {
        let command = stdout_flood_command();
        let limits = ProcessLimits::new(Duration::from_secs(2), 1024, 2048);
        let error = run_bounded(command, b"", limits, "stdout flood mutation")
            .expect_err("stdout flood must exceed its ceiling");
        assert!(error.contains("stdout flood mutation"));
        assert!(error.contains("stdout"));
        assert!(error.contains("1024-byte ceiling"), "{error}");
    }

    #[test]
    fn stderr_flood_uses_its_own_ceiling_and_bounded_diagnostic() {
        let command = stderr_flood_command();
        let limits = ProcessLimits::new(Duration::from_secs(2), 1024, 8192);
        let error = run_bounded(command, b"", limits, "stderr flood mutation")
            .expect_err("stderr flood must exceed its ceiling");
        assert!(error.contains("stderr flood mutation"));
        assert!(
            error.contains("stderr exceeded its 8192-byte ceiling"),
            "{error}"
        );
        assert!(error.contains("... (8193 bytes total)"));
        assert!(
            error.len() < 5000,
            "diagnostic escaped its bounded excerpt: {} bytes",
            error.len()
        );
    }

    #[test]
    fn kills_descendant_that_holds_inherited_output_open() {
        let temp = tempfile::tempdir().expect("temporary pid directory");
        let pid_file = temp.path().join("descendant.pid");
        let cleanup_path = pid_file.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let started = Instant::now();

        let worker = thread::spawn(move || {
            let command = inherited_output_descendant_command(&pid_file);
            let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);
            let result = run_bounded(command, b"", limits, "inherited output mutation");
            let _ = result_tx.send(result);
        });

        let result = match result_rx.recv_timeout(descendant_test_guard()) {
            Ok(result) => result,
            Err(error) => panic!("runner escaped its wall-clock bound: {error}"),
        };
        worker.join().expect("bounded runner worker");

        let error = result.expect_err("inherited output holder must force tree termination");
        assert!(error.contains("inherited output mutation"));
        assert!(error.contains("deadline"));
        assert!(
            started.elapsed() < descendant_test_guard(),
            "process-tree cleanup exceeded the test bound"
        );
        let descendant_pid = read_descendant_pid(&cleanup_path)
            .expect("descendant must publish its pid before inheriting output");
        if !wait_for_descendant_exit(descendant_pid) {
            panic!("descendant {descendant_pid} survived process-tree termination");
        }
    }

    #[test]
    fn kills_quiet_descendant_after_normal_parent_exit() {
        let temp = tempfile::tempdir().expect("temporary pid directory");
        let pid_file = temp.path().join("quiet-descendant.pid");
        let command = quiet_descendant_command(&pid_file);
        let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);

        let output = run_bounded(command, b"", limits, "normal completion mutation")
            .expect("normal parent completion must retain its outcome");

        assert!(output.status.success());
        assert_eq!(output.stdout, b"parent-ready");
        let descendant_pid =
            read_descendant_pid(&pid_file).expect("quiet descendant must publish its pid");
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "quiet descendant {descendant_pid} survived normal completion cleanup"
        );
    }

    #[test]
    fn preserves_child_outcome_when_stdin_closes_early() {
        let command = early_exit_command();
        let stdin = vec![b'x'; 8 * 1024 * 1024];
        let limits = ProcessLimits::new(Duration::from_secs(2), 1024, 1024);

        let output = run_bounded(command, &stdin, limits, "early stdin close mutation")
            .expect("BrokenPipe must not hide an observed child outcome");

        assert_eq!(output.status.code(), Some(23));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn eperm_is_quiescent_only_on_macos_after_all_io_is_complete() {
        let error = std::io::Error::from_raw_os_error(libc::EPERM);

        assert!(!process_tree_absent(&error));
        #[cfg(target_os = "macos")]
        assert!(process_tree_quiescent_after_io(&error));
        #[cfg(not(target_os = "macos"))]
        assert!(!process_tree_quiescent_after_io(&error));
    }
}
