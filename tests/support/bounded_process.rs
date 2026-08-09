use std::ffi::OsStr;
use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
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

pub fn run_bounded(
    command: &mut Command,
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
    let command_display = display_command(command);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{context}: spawn {command_display}: {error}"))?;

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("{context}: {command_display}: child stdin was not piped"))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{context}: {command_display}: child stdout was not piped"))?;
    let child_stderr = child
        .stderr
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
    let status = loop {
        if let Ok(stream) = overflow_rx.try_recv() {
            early_failure = Some(format!(
                "{} exceeded its {}-byte ceiling",
                stream.name(),
                stream_limit(limits, stream)
            ));
            break kill_and_reap(&mut child, context, &command_display)?;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                early_failure = Some(format!("deadline of {:?} expired", limits.timeout));
                break kill_and_reap(&mut child, context, &command_display)?;
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                let reap = kill_and_reap(&mut child, context, &command_display);
                return Err(format!(
                    "{context}: poll {command_display}: {error}; kill/reap: {reap:?}"
                ));
            }
        }
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
    stdin_result.map_err(|error| {
        failure_diagnostic(
            context,
            &command_display,
            &format!("write stdin: {error}"),
            &status,
            &stdout,
            &stderr,
        )
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

fn kill_and_reap(
    child: &mut std::process::Child,
    context: &str,
    command: &str,
) -> Result<ExitStatus, String> {
    let kill_error = child.kill().err();
    child.wait().map_err(|wait_error| {
        format!(
            "{context}: failed to reap {command} after kill; kill={kill_error:?}; wait={wait_error}"
        )
    })
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
