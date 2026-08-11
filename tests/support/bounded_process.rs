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

/// Where a reported exit status came from.
///
/// A status the harness reaped after terminating the process tree is not an
/// ending the child chose: on Windows `TerminateJobObject` supplies the exit
/// code itself, and on Unix the group kill supplies the signal. Rendering that
/// the same way as a child-chosen exit makes a timeout indistinguishable from a
/// child that died instantly.
#[derive(Clone, Copy)]
enum StatusOrigin {
    ChildExit,
    HarnessTermination,
}

/// A child's exit status together with the origin that explains it.
#[derive(Clone, Copy)]
struct Ending {
    status: ExitStatus,
    origin: StatusOrigin,
}

impl Ending {
    fn child_exit(status: ExitStatus) -> Self {
        Self {
            status,
            origin: StatusOrigin::ChildExit,
        }
    }

    fn harness_termination(status: ExitStatus) -> Self {
        Self {
            status,
            origin: StatusOrigin::HarnessTermination,
        }
    }

    fn describe(&self) -> String {
        match self.origin {
            StatusOrigin::ChildExit => format!("{} (child exit)", self.status),
            StatusOrigin::HarnessTermination => format!(
                "{} (recorded after harness termination; may be the termination's own code)",
                self.status
            ),
        }
    }
}

/// Names what the deadline was still waiting for when it expired.
///
/// Without this, "deadline expired" covers two different endings — a child that
/// never exited, and a child that exited while a descendant kept its output
/// handles open — and the reader of a failure cannot tell them apart.
fn deadline_expiry(
    timeout: Duration,
    child_exited: bool,
    stdin_pending: bool,
    stdout_pending: bool,
    stderr_pending: bool,
) -> String {
    let mut outstanding = Vec::new();
    if !child_exited {
        outstanding.push("child still running");
    }
    if stdin_pending {
        outstanding.push("stdin still being written");
    }
    if stdout_pending {
        outstanding.push("stdout handle still open");
    }
    if stderr_pending {
        outstanding.push("stderr handle still open");
    }
    format!(
        "deadline of {timeout:?} expired; outstanding=[{}]",
        outstanding.join(", ")
    )
}

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
    let ending = loop {
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
                    let reap =
                        match terminate_and_reap(child.as_mut(), None, context, &command_display) {
                            Ok(ending) => ending.describe(),
                            Err(reap_error) => reap_error,
                        };
                    return Err(format!(
                        "{context}: poll {command_display}: {error}; terminate/reap: {reap}"
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
            break Ending::child_exit(
                observed_status
                    .take()
                    .expect("status was checked immediately above"),
            );
        }

        if Instant::now() >= deadline {
            if early_failure.is_none() {
                early_failure = Some(deadline_expiry(
                    limits.timeout,
                    observed_status.is_some(),
                    !stdin_writer.is_finished(),
                    !stdout_reader.is_finished(),
                    !stderr_reader.is_finished(),
                ));
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
            &ending,
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
            &ending,
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
            &ending,
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
            &ending,
            &stdout,
            &stderr,
        ))
    })?;

    Ok(Output {
        status: ending.status,
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
) -> Result<Ending, String> {
    let tree_error = child
        .start_kill()
        .err()
        .filter(|error| !process_tree_absent(error));
    let direct_kill_error = tree_error
        .as_ref()
        .and_then(|_| child.inner_mut().start_kill().err());
    let ending = reap_after_tree_kill(child, observed_status).map_err(|reap_error| {
        format!(
            "{context}: failed to reap {command} after tree termination; tree={tree_error:?}; direct_kill={direct_kill_error:?}; reap={reap_error}"
        )
    })?;
    if let Some(tree_error) = tree_error {
        return Err(format!(
            "{context}: failed to terminate process tree for {command}: {tree_error}; direct_kill={direct_kill_error:?}; status={}",
            ending.describe()
        ));
    }
    Ok(ending)
}

fn reap_after_tree_kill(
    child: &mut dyn ChildWrapper,
    observed_status: Option<ExitStatus>,
) -> Result<Ending, String> {
    if let Some(status) = observed_status {
        return Ok(Ending::child_exit(status));
    }

    let deadline = Instant::now() + REAP_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Ending::harness_termination(status)),
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
    ending: &Ending,
    stdout: &[u8],
    stderr: &[u8],
) -> String {
    format!(
        "{context}: {command}: {failure}; status={}; stdout={}; stderr={}",
        ending.describe(),
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
    use super::{deadline_expiry, run_bounded, ProcessLimits};
    #[cfg(unix)]
    use super::{process_tree_absent, process_tree_quiescent_after_io};
    use std::io::Write;
    use std::path::PathBuf;
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

    /// Re-exec mode env; replaces the old environment-derived PID-file path.
    const HELPER_MODE_ENV: &str = "ASSAY_BOUNDED_PROCESS_HELPER_MODE";
    /// libtest filter for the helper. Zero/multiple matches must not go green.
    const HELPER_FILTER: &str = "descendant_spawner_helper_process";
    const READY_RECORD_PREFIX: &str = "READY pid=";
    /// Solo control marker: no pid prefix, so mistaking Solo for a descendant fails parse.
    const READY_SOLO_RECORD: &str = "READY solo";
    const MAX_PID_DIGITS: usize = 10;

    fn ready_record(pid: u32) -> String {
        format!("{READY_RECORD_PREFIX}{pid}")
    }

    /// Strict `READY pid=<nonzero u32>` parse. Identical repeats agree; disagree errs.
    fn descendant_pid_from(record: &str) -> Result<u32, String> {
        let mut seen: Option<u32> = None;
        for (start, _) in record.match_indices(READY_RECORD_PREFIX) {
            let digits: String = record[start + READY_RECORD_PREFIX.len()..]
                .chars()
                .take(MAX_PID_DIGITS + 1)
                .take_while(char::is_ascii_digit)
                .collect();
            if digits.is_empty() {
                continue;
            }
            let pid = match digits.parse::<u32>() {
                Ok(0) | Err(_) => {
                    return Err(format!("readiness record has no usable pid: {record:?}"));
                }
                Ok(pid) => pid,
            };
            match seen {
                Some(first) if first != pid => {
                    return Err(format!("readiness records disagree: {first} and {pid}"));
                }
                _ => seen = Some(pid),
            }
        }
        seen.ok_or_else(|| format!("no readiness record: {record:?}"))
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum HelperPlan {
        Inherited,
        Detached,
        Solo,
    }

    impl HelperPlan {
        fn as_str(self) -> &'static str {
            match self {
                Self::Inherited => "inherited",
                Self::Detached => "detached",
                Self::Solo => "solo",
            }
        }

        fn parse(value: &str) -> Option<Self> {
            match value {
                "inherited" => Some(Self::Inherited),
                "detached" => Some(Self::Detached),
                "solo" => Some(Self::Solo),
                _ => None,
            }
        }
    }

    fn current_test_binary() -> PathBuf {
        std::env::current_exe().expect("the running test binary must be addressable")
    }

    /// Windows: re-exec this binary (PowerShell startup was observed ~18s / 5s bound).
    #[cfg(windows)]
    fn descendant_spawner_command(plan: HelperPlan) -> Command {
        let mut command = Command::new(current_test_binary());
        command
            .args([
                HELPER_FILTER,
                "--ignored",
                "--nocapture",
                "--format",
                "terse",
            ])
            .env(HELPER_MODE_ENV, plan.as_str());
        command
    }

    /// Unix: shell spawner emitting the same READY records as the Windows helper.
    #[cfg(unix)]
    fn descendant_spawner_command(plan: HelperPlan) -> Command {
        let script = match plan {
            HelperPlan::Solo => {
                format!("printf '{READY_SOLO_RECORD}'; printf '{READY_SOLO_RECORD}' >&2")
            }
            HelperPlan::Inherited | HelperPlan::Detached => {
                let detach = match plan {
                    HelperPlan::Detached => "exec </dev/null >/dev/null 2>&1;",
                    _ => "",
                };
                format!(
                    "sh -c '{detach} while :; do :; done' descendant & \
                     printf '{READY_RECORD_PREFIX}%s' \"$!\"; \
                     printf '{READY_RECORD_PREFIX}%s' \"$!\" >&2"
                )
            }
        };
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    #[allow(clippy::zombie_processes)]
    #[test]
    #[ignore = "re-executed as the bounded child of the descendant tests"]
    fn descendant_spawner_helper_process() {
        let mode = std::env::var(HELPER_MODE_ENV)
            .unwrap_or_else(|error| panic!("{HELPER_MODE_ENV} must be set: {error}"));
        let plan =
            HelperPlan::parse(&mode).unwrap_or_else(|| panic!("unknown helper plan {mode:?}"));

        let record = match plan {
            HelperPlan::Solo => READY_SOLO_RECORD.to_owned(),
            HelperPlan::Inherited | HelperPlan::Detached => {
                let mut descendant = hanging_command();
                descendant.stdin(Stdio::null());
                if plan == HelperPlan::Detached {
                    descendant.stdout(Stdio::null()).stderr(Stdio::null());
                }
                let child = descendant
                    .spawn()
                    .unwrap_or_else(|error| panic!("spawn the descendant: {error}"));
                ready_record(child.id())
            }
        };

        std::io::stdout()
            .write_all(record.as_bytes())
            .expect("report readiness on stdout");
        std::io::stdout().flush().expect("flush readiness");
        std::io::stderr()
            .write_all(record.as_bytes())
            .expect("report readiness on stderr");
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

    #[test]
    fn liveness_probe_recognizes_the_current_test_process() {
        assert!(
            descendant_is_alive(std::process::id()),
            "liveness probe must distinguish a live process from an exited descendant"
        );
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

    #[test]
    fn deadline_expiry_names_each_outstanding_wait_and_distinguishes_child_liveness() {
        let all = deadline_expiry(Duration::from_secs(5), false, true, true, true);
        assert!(
            all.contains("outstanding=["),
            "deadline naming must expose outstanding waits: {all}"
        );
        assert!(all.contains("child still running"), "{all}");
        assert!(all.contains("stdin still being written"), "{all}");
        assert!(all.contains("stdout handle still open"), "{all}");
        assert!(all.contains("stderr handle still open"), "{all}");

        let running = deadline_expiry(Duration::from_secs(5), false, false, true, true);
        assert!(running.contains("child still running"), "{running}");
        assert!(running.contains("stdout handle still open"), "{running}");
        assert!(running.contains("stderr handle still open"), "{running}");

        let exited = deadline_expiry(Duration::from_secs(5), true, false, true, true);
        assert!(!exited.contains("child still running"), "{exited}");
        assert!(exited.contains("stdout handle still open"), "{exited}");
        assert!(exited.contains("stderr handle still open"), "{exited}");
        assert!(
            exited.contains("outstanding=["),
            "child-exited-with-open-handles must still name outstanding waits: {exited}"
        );
    }

    #[test]
    fn a_harness_terminated_status_is_not_reported_as_a_child_exit() {
        let command = hanging_command();
        let limits = ProcessLimits::new(Duration::from_millis(100), 1024, 1024);
        let error = run_bounded(command, b"", limits, "harness termination mutation")
            .expect_err("hanging child must time out");

        assert!(
            error.contains("outstanding=[child still running"),
            "{error}"
        );
        assert!(
            error.contains("recorded after harness termination"),
            "{error}"
        );
        assert!(!error.contains("(child exit)"), "{error}");
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
        let (result_tx, result_rx) = mpsc::channel();
        let started = Instant::now();

        let worker = thread::spawn(move || {
            let command = descendant_spawner_command(HelperPlan::Inherited);
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
        let descendant_pid = descendant_pid_from(&error)
            .unwrap_or_else(|reason| panic!("descendant pid unreadable: {reason}"));
        if !wait_for_descendant_exit(descendant_pid) {
            panic!("descendant {descendant_pid} survived process-tree termination");
        }
    }

    /// Unix may keep success; Windows deadline only if Solo (same head) proves EOF.
    #[cfg(unix)]
    #[test]
    fn kills_quiet_descendant_after_normal_parent_exit() {
        let command = descendant_spawner_command(HelperPlan::Detached);
        let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);

        let output = run_bounded(command, b"", limits, "normal completion mutation")
            .expect("normal parent completion must retain its outcome");

        assert!(output.status.success());
        let stderr = String::from_utf8(output.stderr.clone()).expect("stderr is the child's text");
        let descendant_pid = descendant_pid_from(&stderr)
            .unwrap_or_else(|reason| panic!("quiet descendant pid unreadable: {reason}"));
        assert_eq!(stderr, ready_record(descendant_pid));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&ready_record(descendant_pid)),
            "stdout must carry READY: {stdout:?}"
        );
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "quiet descendant {descendant_pid} survived normal completion cleanup"
        );
    }

    #[cfg(windows)]
    #[test]
    fn kills_quiet_descendant_after_normal_parent_exit() {
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let command = descendant_spawner_command(HelperPlan::Detached);
            let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);
            let _ = result_tx.send(run_bounded(
                command,
                b"",
                limits,
                "normal completion mutation",
            ));
        });
        let result = match result_rx.recv_timeout(descendant_test_guard()) {
            Ok(result) => result,
            Err(error) => panic!("runner escaped its wall-clock bound: {error}"),
        };
        worker.join().expect("bounded runner worker");

        let error = result.expect_err(
            "a Windows descendant inherits the run's handles, so the deadline is the only \
             reachable ending; a success here means the platform changed",
        );
        assert!(error.contains("normal completion mutation"));
        assert!(error.contains("(child exit)"), "{error}");
        assert!(!error.contains("child still running"), "{error}");
        assert!(error.contains("stdout handle still open"), "{error}");
        assert!(error.contains("stderr handle still open"), "{error}");
        let descendant_pid = descendant_pid_from(&error)
            .unwrap_or_else(|reason| panic!("quiet descendant pid unreadable: {reason}"));
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "quiet descendant {descendant_pid} survived process-tree termination"
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

    #[test]
    fn the_helper_plan_survives_the_round_trip_to_the_helper() {
        for plan in [
            HelperPlan::Inherited,
            HelperPlan::Detached,
            HelperPlan::Solo,
        ] {
            assert_eq!(HelperPlan::parse(plan.as_str()), Some(plan));
        }
        assert_eq!(HelperPlan::parse("other"), None);
        assert!(descendant_pid_from(READY_SOLO_RECORD).is_err());
    }

    #[test]
    fn the_re_executed_helper_is_selected_by_exactly_one_filter_match() {
        let output = Command::new(current_test_binary())
            .args(["--list", HELPER_FILTER])
            .output()
            .expect("list the helper test");
        assert!(
            output.status.success(),
            "listing the helper test failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let matches: Vec<_> = stdout
            .lines()
            .filter(|line| line.contains(HELPER_FILTER))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "helper filter must select exactly one test, got {}: {stdout}",
            matches.len()
        );
    }

    /// Unix-only helper smoke: Windows exercises re-exec via the descendant tests.
    #[cfg(unix)]
    #[test]
    fn the_re_executed_helper_publishes_a_pid_and_reports_readiness() {
        let mut command = Command::new(current_test_binary());
        command
            .args([
                HELPER_FILTER,
                "--ignored",
                "--nocapture",
                "--format",
                "terse",
            ])
            .env(HELPER_MODE_ENV, HelperPlan::Detached.as_str());
        let limits = ProcessLimits::new(descendant_test_guard(), 1024, 1024);

        let output = run_bounded(command, b"", limits, "helper re-execution")
            .expect("the re-executed helper must complete within the test guard");

        assert!(output.status.success(), "{:?}", output.status);
        let stderr = String::from_utf8(output.stderr.clone()).expect("stderr is the child's text");
        let descendant_pid = descendant_pid_from(&stderr)
            .unwrap_or_else(|reason| panic!("helper pid unreadable: {reason}"));
        assert_eq!(stderr, ready_record(descendant_pid));
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "descendant {descendant_pid} of the re-executed helper survived cleanup"
        );
    }

    /// Solo control: no descendant ⇒ EOF/success. Pins the inherited-handle claim.
    #[test]
    fn the_same_child_reaches_eof_when_it_spawns_no_descendant() {
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let command = descendant_spawner_command(HelperPlan::Solo);
            let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);
            let _ = result_tx.send(run_bounded(command, b"", limits, "no descendant control"));
        });
        let result = match result_rx.recv_timeout(descendant_test_guard()) {
            Ok(result) => result,
            Err(error) => panic!("runner escaped its wall-clock bound: {error}"),
        };
        worker.join().expect("bounded runner worker");
        let output = result.unwrap_or_else(|error| {
            panic!(
                "a child that spawned nothing must reach EOF; without a descendant the \
                 inherited-handle account of #2249 is refuted: {error}"
            )
        });
        assert!(output.status.success(), "{:?}", output.status);
        let stderr = String::from_utf8(output.stderr.clone()).expect("stderr is the child's text");
        assert_eq!(stderr, READY_SOLO_RECORD);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(READY_SOLO_RECORD),
            "stdout must carry the same record: {stdout:?}"
        );
    }

    #[test]
    fn only_one_exactly_shaped_readiness_record_yields_a_pid() {
        assert_eq!(descendant_pid_from("READY pid=4294967295"), Ok(u32::MAX));
        assert_eq!(
            descendant_pid_from("\nrunning 1 test\nREADY pid=1234.\ntest result: ok.\n"),
            Ok(1234),
            "libtest noise must not hide the record"
        );
        assert_eq!(
            descendant_pid_from(
                "sh -c printf 'READY pid=%s'\": deadline expired; \
                 stdout=\"READY pid=77\" (12 bytes); stderr=\"READY pid=77\" (12 bytes)"
            ),
            Ok(77),
            "identical both-channel quotes must still read"
        );

        for rejected in [
            "",
            "READY pid=",
            "READY pid=x",
            "READY pid=-1",
            "READY pid=0",
            "READY pid=4294967296",
            "READY pid=42949672960",
            "READY pid=1 READY pid=2",
            "READY pid=1 READY pid=1 READY pid=3",
            "parent-ready",
            "READY solo",
        ] {
            assert!(
                descendant_pid_from(rejected).is_err(),
                "{rejected:?} must not yield a pid"
            );
        }
    }
}
