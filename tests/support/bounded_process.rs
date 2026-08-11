use std::ffi::OsStr;
use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const REAP_GRACE: Duration = Duration::from_secs(1);
const MAX_DIAGNOSTIC_BYTES: usize = 4096;

// Test-only: when set, every tree-kill attempt returns this error without
// sending a tree kill, so the first attempt and the deadline retry can both
// be exercised under a persistent failure.
#[cfg(test)]
thread_local! {
    static INJECT_TERMINATE_REMAINING_TREE_ERROR: Cell<Option<&'static str>> = const { Cell::new(None) };
}

// Test-only: process id observed when an injected tree-kill failure fires.
#[cfg(test)]
static INJECTED_TREE_KILL_CHILD_PID: AtomicU32 = AtomicU32::new(0);

// Test-only: how many injected tree-kill failures fired on this thread's run.
#[cfg(test)]
static INJECTED_TREE_KILL_HITS: AtomicUsize = AtomicUsize::new(0);

// Test-only: when set on the `run_bounded` caller thread, spawned I/O workers
// register in `LIVE_IO_WORKERS` for the duration of their closure.
#[cfg(test)]
thread_local! {
    static TRACK_IO_WORKERS: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
static LIVE_IO_WORKERS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
struct IoWorkerGuard;

#[cfg(test)]
impl IoWorkerGuard {
    fn enter() -> Self {
        LIVE_IO_WORKERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(test)]
impl Drop for IoWorkerGuard {
    fn drop(&mut self) {
        LIVE_IO_WORKERS.fetch_sub(1, Ordering::SeqCst);
    }
}

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
    #[cfg(test)]
    let track_io_workers = TRACK_IO_WORKERS.get();
    let stdin_writer = thread::spawn(move || {
        #[cfg(test)]
        let _worker_guard = track_io_workers.then(IoWorkerGuard::enter);
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
    let mut tree_termination_error = None;
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
                Ok(Some(status)) => {
                    observed_status = Some(status);
                    // Descendants that still hold inherited pipes keep readers
                    // from EOF. Terminate the remaining tree now — before
                    // treating reader EOF as the success condition — then drain
                    // buffered output within the same total deadline.
                    let io_outstanding = !stdin_writer.is_finished()
                        || !stdout_reader.is_finished()
                        || !stderr_reader.is_finished();
                    if io_outstanding {
                        // Preserve a tree-kill failure, but do not return while
                        // JoinHandles and descendants may still be live. Keep
                        // draining under the same deadline; terminate_and_reap
                        // below retries termination before joins.
                        if let Err(error) =
                            terminate_remaining_tree(child.as_mut(), context, &command_display)
                        {
                            tree_termination_error = Some(error);
                        }
                    }
                }
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
            // After I/O completion this may accept post-IO quiescence (including
            // macOS EPERM on an empty group) that terminate_remaining_tree does
            // not. When the early path already killed the tree, this is a
            // no-op or that quiescent acceptance.
            finish_process_tree(child.as_mut(), context, &command_display)?;
            break observed_status
                .take()
                .expect("status was checked immediately above");
        }

        if Instant::now() >= deadline {
            if early_failure.is_none() && tree_termination_error.is_none() {
                early_failure = Some(format!("deadline of {:?} expired", limits.timeout));
            }
            // Never return here on terminate/reap failure: JoinHandles and
            // descendants may still be live. Preserve the first error, best-
            // effort kill so readers can EOF, then join below before surfacing.
            let prior_status = observed_status.take();
            let status = match terminate_and_reap(
                child.as_mut(),
                prior_status,
                context,
                &command_display,
            ) {
                Ok(status) => status,
                Err(error) => {
                    if tree_termination_error.is_none() {
                        tree_termination_error = Some(error);
                    }
                    // terminate_and_reap may have skipped OS kill under a
                    // persistent injected failure; a direct start_kill is
                    // outside that seam so joins below stay bounded.
                    let _ = child.start_kill();
                    let _ = child.inner_mut().start_kill();
                    if let Some(status) = prior_status {
                        status
                    } else {
                        reap_after_tree_kill(child.as_mut(), None).map_err(|reap_error| {
                            match tree_termination_error.take() {
                                Some(error) => format!("{error}; reap={reap_error}"),
                                None => format!(
                                    "{context}: failed to reap {command_display} after deadline termination: {reap_error}"
                                ),
                            }
                        })?
                    }
                }
            };
            break status;
        }
        thread::sleep(POLL_INTERVAL);
    };

    let stdin_result = stdin_writer
        .join()
        .map_err(|_| format!("{context}: {command_display}: stdin writer panicked"))?;
    let stdout = join_reader(stdout_reader, context, &command_display, "stdout")?;
    let stderr = join_reader(stderr_reader, context, &command_display, "stderr")?;

    if let Some(error) = tree_termination_error {
        return Err(format!(
            "{error}; status={status}; stdout={}; stderr={}",
            excerpt(&stdout),
            excerpt(&stderr),
        ));
    }

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
    #[cfg(test)]
    let track_io_workers = TRACK_IO_WORKERS.get();
    thread::spawn(move || {
        #[cfg(test)]
        let _worker_guard = track_io_workers.then(IoWorkerGuard::enter);
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
    #[cfg(test)]
    if let Some(injected) = injected_tree_kill_failure(child.id()) {
        // Persistent injected failure skips the OS kill, matching
        // terminate_remaining_tree, so the deadline retry can still return Err
        // while descendants hold inherited pipes.
        let status = reap_after_tree_kill(child, observed_status).map_err(|reap_error| {
            format!(
                "{context}: failed to reap {command} after tree termination; tree=Some({injected}); direct_kill=None; reap={reap_error}"
            )
        })?;
        return Err(format!(
            "{context}: failed to terminate process tree for {command}: {injected}; direct_kill=None; status={status}"
        ));
    }
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

/// Terminates descendants that outlived the direct child so inherited pipes can
/// reach EOF. Only demonstrably absent trees are accepted here
/// (`process_tree_absent`); macOS EPERM is not, because I/O may still be
/// outstanding and that error is reserved for the post-IO finish path.
fn terminate_remaining_tree(
    child: &mut dyn ChildWrapper,
    context: &str,
    command: &str,
) -> Result<(), String> {
    #[cfg(test)]
    if let Some(injected) = injected_tree_kill_failure(child.id()) {
        return Err(format!(
            "{context}: failed to terminate remaining process tree for {command}: {injected}"
        ));
    }
    match child.start_kill() {
        Ok(()) => Ok(()),
        Err(error) if process_tree_absent(&error) => Ok(()),
        Err(error) => {
            let direct_kill_error = child.inner_mut().start_kill().err();
            Err(format!(
                "{context}: failed to terminate remaining process tree for {command}: {error}; direct_kill={direct_kill_error:?}"
            ))
        }
    }
}

/// Test-only: observe a persistent injected tree-kill failure without clearing it.
///
/// Unlike a one-shot `take()`, this stays armed across the early
/// `terminate_remaining_tree` attempt and the deadline `terminate_and_reap`
/// retry so both arms can fail the same way.
#[cfg(test)]
fn injected_tree_kill_failure(child_id: u32) -> Option<&'static str> {
    INJECT_TERMINATE_REMAINING_TREE_ERROR.get().inspect(|_| {
        INJECTED_TREE_KILL_CHILD_PID.store(child_id, Ordering::SeqCst);
        INJECTED_TREE_KILL_HITS.fetch_add(1, Ordering::SeqCst);
    })
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
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    /// Selected on the bounded child so this test binary, re-executed, acts as
    /// the descendant spawner instead of running the suite.
    const HELPER_MODE_ENV: &str = "ASSAY_BOUNDED_PROCESS_HELPER_MODE";
    /// libtest filter that selects the helper in every binary including this
    /// module, whatever module path it is included under. A filter that matched
    /// nothing would exit 0 silently, so
    /// `the_re_executed_helper_is_selected_by_exactly_one_filter_match` pins it.
    const HELPER_FILTER: &str = "descendant_spawner_helper_process";
    /// Readiness and the descendant's pid travel as one record on both output
    /// channels, so the bounded run can assert one verbatim and find it in the
    /// other. Carrying the pid in already-captured output removes the
    /// environment-controlled pid-file write channel.
    const READY_RECORD_PREFIX: &str = "READY pid=";
    /// The control's record. It names no pid because the control spawns nothing,
    /// and it deliberately does not carry the pid prefix.
    const READY_SOLO_RECORD: &str = "READY solo";
    /// `u32::MAX` is ten digits, so anything longer is not a pid.
    const MAX_PID_DIGITS: usize = 10;

    fn ready_record(pid: u32) -> String {
        format!("{READY_RECORD_PREFIX}{pid}")
    }

    /// Reads the descendant's pid out of a bounded run's captured record.
    ///
    /// The text is a child's output, or a diagnostic quoting it, so it is
    /// untrusted and this rejects rather than scans. Only digits may follow the
    /// prefix and they must be a non-zero `u32`; every record present must name
    /// the same pid; and no record at all is an error.
    fn descendant_pid_from(record: &str) -> Result<u32, String> {
        let mut seen: Option<u32> = None;
        for (start, _) in record.match_indices(READY_RECORD_PREFIX) {
            let digits: String = record[start + READY_RECORD_PREFIX.len()..]
                .chars()
                .take(MAX_PID_DIGITS + 1)
                .take_while(char::is_ascii_digit)
                .collect();
            if digits.is_empty() {
                // The quoted command line carries the prefix with no pid behind
                // it. That is not a record, and not a malformed one either.
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

    /// What the bounded child does before it exits.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum HelperPlan {
        /// Spawns a descendant that keeps stdout and stderr open after its
        /// parent exits, so the reader threads never reach EOF until the tree
        /// is terminated.
        Inherited,
        /// Spawns a descendant that asks for neither handle.
        Detached,
        /// Spawns nothing. The control: with no descendant to hold anything, the
        /// readers must reach EOF when the child exits.
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

    /// The bounded child that spawns a descendant and publishes its pid.
    ///
    /// On Windows this re-executes the test binary rather than scripting
    /// `powershell.exe`, whose startup has been observed longer than the
    /// bounded window. Unix keeps a shell whose startup is a fraction of that
    /// bound; both sides speak the same READY record protocol.
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

    #[cfg(unix)]
    fn descendant_spawner_command(plan: HelperPlan) -> Command {
        let script = match plan {
            HelperPlan::Solo => {
                format!("printf '{READY_SOLO_RECORD}'; printf '{READY_SOLO_RECORD}' >&2",)
            }
            HelperPlan::Inherited | HelperPlan::Detached => {
                let detach = match plan {
                    HelperPlan::Detached => "exec </dev/null >/dev/null 2>&1;",
                    _ => "",
                };
                format!(
                    "sh -c '{detach} while :; do :; done' descendant & \
                     printf '{READY_RECORD_PREFIX}%s' \"$!\"; \
                     printf '{READY_RECORD_PREFIX}%s' \"$!\" >&2",
                )
            }
        };
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    /// Runs as the bounded child of the Windows descendant tests, never as part
    /// of the suite. `--ignored` keeps it out of a normal run.
    // The descendant is deliberately never waited on: it has to outlive this
    // process so that the process-tree termination under test is what reaps it.
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

        // libtest prefixes this process's stdout with its own report, so the
        // record goes to both: stderr carries this child's bytes alone.
        std::io::stdout()
            .write_all(record.as_bytes())
            .expect("report readiness on stdout");
        std::io::stdout().flush().expect("flush readiness");
        std::io::stderr()
            .write_all(record.as_bytes())
            .expect("report readiness on stderr");
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
        assert_eq!(HelperPlan::parse("hidden"), None);
        assert!(descendant_pid_from(READY_SOLO_RECORD).is_err());
    }

    #[test]
    fn the_re_executed_helper_is_selected_by_exactly_one_filter_match() {
        let listing = Command::new(current_test_binary())
            .args([HELPER_FILTER, "--ignored", "--list", "--format", "terse"])
            .output()
            .expect("list the helper test");
        assert!(
            listing.status.success(),
            "listing the helper test failed: {}",
            listing.status
        );
        let listed = String::from_utf8_lossy(&listing.stdout);
        let selected: Vec<&str> = listed
            .lines()
            .filter(|line| line.ends_with(": test"))
            .collect();
        assert_eq!(
            selected.len(),
            1,
            "{HELPER_FILTER} must select exactly one test, listing was {listed:?}"
        );
        assert!(selected[0].contains(HELPER_FILTER), "{listed:?}");
    }

    #[test]
    fn only_one_exactly_shaped_readiness_record_yields_a_pid() {
        assert_eq!(descendant_pid_from("READY pid=4294967295"), Ok(u32::MAX));
        assert_eq!(
            descendant_pid_from("\nrunning 1 test\nREADY pid=1234.\ntest result: ok.\n"),
            Ok(1234),
            "libtest's own report surrounds the record and must not hide it"
        );
        assert_eq!(
            descendant_pid_from(
                "sh -c printf 'READY pid=%s'\": deadline expired; \
                 stdout=\"READY pid=77\" (12 bytes); stderr=\"READY pid=77\" (12 bytes)"
            ),
            Ok(77),
            "one pid written to both channels and quoted back must still read"
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
        ] {
            assert!(
                descendant_pid_from(rejected).is_err(),
                "{rejected:?} must not yield a pid"
            );
        }
    }

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

    /// A probe that cannot run is not an answer, so a spawn failure panics
    /// rather than reporting a descendant dead.
    #[cfg(unix)]
    fn descendant_is_alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("the liveness probe must be runnable")
            .success()
    }

    /// `tasklist` is native. PowerShell must stay out of the bounded window.
    #[cfg(windows)]
    fn descendant_is_alive(pid: u32) -> bool {
        let listing = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stderr(Stdio::null())
            .output()
            .expect("the liveness probe must be runnable");
        let pid = pid.to_string();
        String::from_utf8_lossy(&listing.stdout)
            .split_whitespace()
            .any(|field| field == pid)
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

    fn assert_parent_exit_preserves_ready_and_kills_descendant(plan: HelperPlan, context: &str) {
        let (result_tx, result_rx) = mpsc::channel();
        let started = Instant::now();
        let context_for_worker = context.to_owned();

        let worker = thread::spawn(move || {
            let command = descendant_spawner_command(plan);
            let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);
            let _ = result_tx.send(run_bounded(command, b"", limits, &context_for_worker));
        });

        let result = match result_rx.recv_timeout(descendant_test_guard()) {
            Ok(result) => result,
            Err(error) => panic!("runner escaped its wall-clock bound: {error}"),
        };
        worker.join().expect("bounded runner worker");

        let output = result.unwrap_or_else(|error| {
            panic!(
                "{context}: direct child exit with a live descendant must return Ok before the \
                 existing deadline, preserve parent status/READY bytes, and leave the descendant \
                 dead; got Err: {error}"
            )
        });

        assert!(
            output.status.success(),
            "{context}: preserved parent status must be success, got {:?}",
            output.status
        );
        let stderr = String::from_utf8(output.stderr.clone()).expect("stderr is the child's text");
        let descendant_pid = descendant_pid_from(&stderr)
            .unwrap_or_else(|reason| panic!("{context}: descendant pid unreadable: {reason}"));
        assert_eq!(stderr, ready_record(descendant_pid));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            descendant_pid_from(&stdout).ok(),
            Some(descendant_pid),
            "{context}: stdout must carry the same readiness record: {stdout:?}"
        );
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "{context}: descendant {descendant_pid} survived process-tree termination"
        );
        assert!(
            started.elapsed() < descendant_test_guard(),
            "{context}: process-tree cleanup exceeded the test bound"
        );
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

    /// Contract: once the direct child exits, a descendant that still holds the
    /// run's inherited stdout/stderr must not force a deadline. `run_bounded`
    /// returns Ok with the parent's status and READY bytes, and the descendant
    /// is dead — on every platform, with no Windows-expects-deadline split.
    #[test]
    fn parent_exit_with_inherited_output_descendant_returns_ok_before_deadline() {
        assert_parent_exit_preserves_ready_and_kills_descendant(
            HelperPlan::Inherited,
            "inherited output mutation",
        );
    }

    /// Quiet-success contract: a descendant that asked for null stdio is still
    /// terminated after a normal parent exit, and the run remains Ok with READY
    /// bytes preserved. Same assertion on Windows and Unix — no platform split
    /// that accepts the defect.
    #[test]
    fn parent_exit_with_quiet_descendant_returns_ok_before_deadline() {
        assert_parent_exit_preserves_ready_and_kills_descendant(
            HelperPlan::Detached,
            "normal completion mutation",
        );
    }

    /// Solo control for the inherited-handle claim: same binary/re-exec/shell
    /// path, same limits and guard, but no descendant. Success here locates any
    /// EOF failure in the descendant arm; a deadline here would refute that
    /// account, because nothing exists to hold the handles.
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
                "a child that spawned nothing must reach EOF; that it did not refutes the \
                 inherited-handle account of #2249, since no descendant existed to hold the \
                 run's handles: {error}"
            )
        });

        assert!(output.status.success(), "{:?}", output.status);
        let stderr = String::from_utf8(output.stderr.clone()).expect("stderr is the child's text");
        assert_eq!(
            stderr, READY_SOLO_RECORD,
            "the control must be the same child, reporting"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(READY_SOLO_RECORD),
            "stdout must carry the same record: {stdout:?}"
        );
    }

    /// On Windows the descendant tests use this re-execution as their bounded
    /// child. Here it separates "the helper is broken" from "the bound was
    /// lost": this bound is the test guard, not the deadline under test.
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

    /// Contract: a persistent tree-kill failure (first attempt and deadline
    /// retry) must not let `run_bounded` return while I/O workers or
    /// process-tree descendants are still live. The original termination error
    /// is preserved after bounded cleanup and joins.
    #[test]
    fn early_tree_kill_failure_preserves_error_after_bounded_cleanup() {
        use super::{
            INJECTED_TREE_KILL_CHILD_PID, INJECTED_TREE_KILL_HITS,
            INJECT_TERMINATE_REMAINING_TREE_ERROR, LIVE_IO_WORKERS, TRACK_IO_WORKERS,
        };
        use std::sync::atomic::Ordering;

        const INJECTED: &str = "injected tree-kill failure";

        struct TrackingGuard;
        impl Drop for TrackingGuard {
            fn drop(&mut self) {
                TRACK_IO_WORKERS.set(false);
                INJECT_TERMINATE_REMAINING_TREE_ERROR.set(None);
                INJECTED_TREE_KILL_HITS.store(0, Ordering::SeqCst);
                let pid = INJECTED_TREE_KILL_CHILD_PID.swap(0, Ordering::SeqCst);
                best_effort_kill_tree_for_test(pid);
            }
        }
        let _tracking_guard = TrackingGuard;
        INJECTED_TREE_KILL_CHILD_PID.store(0, Ordering::SeqCst);
        INJECTED_TREE_KILL_HITS.store(0, Ordering::SeqCst);
        assert_eq!(LIVE_IO_WORKERS.load(Ordering::SeqCst), 0);

        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            TRACK_IO_WORKERS.set(true);
            INJECT_TERMINATE_REMAINING_TREE_ERROR.set(Some(INJECTED));
            let command = descendant_spawner_command(HelperPlan::Inherited);
            let limits = ProcessLimits::new(descendant_run_timeout(), 1024, 1024);
            let _ = result_tx.send(run_bounded(
                command,
                b"",
                limits,
                "injected tree-kill failure mutation",
            ));
        });

        let result = match result_rx.recv_timeout(descendant_test_guard()) {
            Ok(result) => result,
            Err(error) => panic!("runner escaped its wall-clock bound: {error}"),
        };
        worker.join().expect("bounded runner worker");

        let live_after_return = LIVE_IO_WORKERS.load(Ordering::SeqCst);
        let injected_child_pid = INJECTED_TREE_KILL_CHILD_PID.load(Ordering::SeqCst);
        let injected_hits = INJECTED_TREE_KILL_HITS.load(Ordering::SeqCst);

        let error = result
            .expect_err("persistent injected tree-kill failure must surface as Err after cleanup");
        assert!(
            injected_hits >= 2,
            "injection must arm both the early terminate_remaining_tree attempt and the \
             deadline terminate_and_reap retry; hits={injected_hits}; error={error}"
        );
        assert!(
            error.contains(INJECTED),
            "original termination error must be preserved: {error}"
        );
        assert!(
            error.contains("failed to terminate remaining process tree"),
            "first tree-kill failure wording must be preserved: {error}"
        );
        assert_eq!(
            live_after_return, 0,
            "run_bounded returned while {live_after_return} I/O worker(s) still live \
             (detached JoinHandles); injected child pid={injected_child_pid}; \
             hits={injected_hits}; error={error}"
        );

        let descendant_pid = descendant_pid_from(&error).unwrap_or_else(|reason| {
            panic!("descendant pid must be recoverable after cleanup joins: {reason}; {error}")
        });
        assert!(
            wait_for_descendant_exit(descendant_pid),
            "descendant {descendant_pid} still live after run_bounded returned; error={error}"
        );
    }

    #[cfg(unix)]
    fn best_effort_kill_tree_for_test(pid: u32) {
        if pid == 0 {
            return;
        }
        // ProcessGroup::leader uses the child pid as the process-group id.
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(windows)]
    fn best_effort_kill_tree_for_test(pid: u32) {
        if pid == 0 {
            return;
        }
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
