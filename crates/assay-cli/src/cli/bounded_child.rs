//! Bounded child wait for production CLI probes.
//!
//! The wait_timeout + kill/reap loop is the pattern from
//! `assay-sim::subprocess` (`ChildExt::wait_timeout` and the timeout arm).
//! That crate's `subprocess_verify` is not reused: it inherits stdin and
//! materializes stderr with `read_to_string` before any cap.
//! `tests/support/bounded_process.rs` stays test-only (Windows JobObject
//! included) and is not imported here.
//!
//! Readers run concurrently and stop at `output_cap`. The same deadline
//! covers child exit and pipe drain, so a full pipe or a descendant that
//! keeps a handle open cannot block past it. Timed-out reader threads are
//! not joined. Timeout still kill()+wait()s the direct child so it is
//! reaped before return; try_wait after kill can leave it unreaped.

use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundedChildError {
    NotFound,
    Spawn(io::ErrorKind),
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundedChildOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
}

fn spawn_capped_reader(mut reader: impl Read + Send + 'static, cap: usize) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = vec![0; cap];
        let mut filled = 0;
        while filled < cap {
            match reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        buf.truncate(filled);
        drop(reader);
        let _ = tx.send(buf);
    });
    rx
}

fn take_reader(rx: &Receiver<Vec<u8>>, slot: &mut Option<Vec<u8>>) {
    if slot.is_some() {
        return;
    }
    match rx.try_recv() {
        Ok(bytes) => *slot = Some(bytes),
        Err(TryRecvError::Disconnected) => *slot = Some(Vec::new()),
        Err(TryRecvError::Empty) => {}
    }
}

fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    output_cap: usize,
) -> Result<BoundedChildOutput, BoundedChildError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(BoundedChildError::NotFound);
        }
        Err(error) => return Err(BoundedChildError::Spawn(error.kind())),
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_rx = spawn_capped_reader(
        stdout.ok_or(BoundedChildError::Spawn(io::ErrorKind::BrokenPipe))?,
        output_cap,
    );
    let stderr_rx = spawn_capped_reader(
        stderr.ok_or(BoundedChildError::Spawn(io::ErrorKind::BrokenPipe))?,
        output_cap,
    );

    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;

    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) => status = Some(exit),
                Ok(None) => {}
                Err(error) => {
                    kill_and_reap(&mut child);
                    return Err(BoundedChildError::Spawn(error.kind()));
                }
            }
        }

        take_reader(&stdout_rx, &mut stdout);
        take_reader(&stderr_rx, &mut stderr);

        if let (Some(status), Some(stdout), Some(_stderr)) =
            (status, stdout.as_ref(), stderr.as_ref())
        {
            return Ok(BoundedChildOutput {
                exit_code: status.code().unwrap_or(1),
                stdout: stdout.clone(),
            });
        }

        let now = Instant::now();
        if now >= deadline {
            kill_and_reap(&mut child);
            return Err(BoundedChildError::Timeout);
        }
        thread::sleep(deadline.saturating_duration_since(now).min(POLL_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Stdio;

    const HELPER_MODE_ENV: &str = "ASSAY_BOUNDED_CHILD_HELPER_MODE";
    const HELPER_FILTER: &str = "cli::bounded_child::tests::bounded_child_helper_process";

    #[test]
    #[ignore = "re-executed as the bounded child of the helper mutations"]
    fn bounded_child_helper_process() {
        match std::env::var(HELPER_MODE_ENV).as_deref() {
            Ok("hang") => loop {
                thread::park();
            },
            Ok("flood") => {
                let chunk = [b'F'; 1024];
                let mut out = std::io::stdout();
                loop {
                    if out.write_all(&chunk).is_err() {
                        break;
                    }
                }
            }
            Ok("hold-pipe") => {
                let mut descendant = helper_command("park");
                descendant.stdin(Stdio::null());
                #[allow(clippy::zombie_processes)]
                let _child = descendant.spawn().expect("spawn pipe-holding descendant");
            }
            Ok("park") => thread::sleep(Duration::from_secs(8)),
            Ok("ok") => {
                std::io::stdout()
                    .write_all(b"ok\n")
                    .expect("write readiness");
            }
            other => panic!("{HELPER_MODE_ENV} must be a known plan, got {other:?}"),
        }
    }

    #[test]
    fn the_re_executed_helper_is_selected_by_exactly_one_filter_match() {
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--list", HELPER_FILTER, "--exact", "--ignored"])
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
            .filter(|line| line.contains("bounded_child_helper_process"))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "helper filter must select exactly one test, got {}: {stdout}",
            matches.len()
        );
    }

    fn helper_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test binary"));
        command
            .args([
                HELPER_FILTER,
                "--exact",
                "--ignored",
                "--nocapture",
                "--format",
                "terse",
            ])
            .env(HELPER_MODE_ENV, mode);
        command
    }

    fn run_under_guard(
        mut command: Command,
        timeout: Duration,
        cap: usize,
        guard: Duration,
    ) -> Result<BoundedChildOutput, BoundedChildError> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(run_bounded(&mut command, timeout, cap));
        });
        rx.recv_timeout(guard)
            .unwrap_or_else(|_| panic!("run_bounded escaped the {guard:?} wall-clock bound"))
    }

    #[test]
    fn missing_binary_is_not_found() {
        let mut command = Command::new("2195-bounded-child-not-on-path");
        let error = run_bounded(&mut command, Duration::from_secs(1), 64).unwrap_err();
        assert_eq!(error, BoundedChildError::NotFound);
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        // SAFETY: signal 0 is a liveness probe for a pid this test spawned.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    #[test]
    fn kill_and_reap_reaps_before_drop() {
        let mut child = helper_command("hang")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("hang helper");
        let pid = child.id();
        kill_and_reap(&mut child);
        assert!(
            child.try_wait().ok().flatten().is_some(),
            "kill+wait must leave pid {pid} already reaped; kill+try_wait can return None"
        );
        #[cfg(unix)]
        assert!(
            !process_exists(pid),
            "kill+try_wait can leave pid {pid} unreaped until Drop"
        );
    }

    #[test]
    fn rust_helper_hang_times_out_without_an_external_sleep() {
        let error = run_under_guard(
            helper_command("hang"),
            Duration::from_millis(400),
            64,
            Duration::from_secs(2),
        )
        .expect_err("a parked helper must hit the deadline");
        assert_eq!(error, BoundedChildError::Timeout);
    }

    #[test]
    fn stdout_flood_stays_cap_bounded_and_returns_before_the_guard() {
        let result = run_under_guard(
            helper_command("flood"),
            Duration::from_secs(2),
            64,
            Duration::from_secs(3),
        );
        match result {
            Ok(output) => {
                assert!(
                    output.stdout.len() <= 64,
                    "flood stdout escaped the cap: {} bytes",
                    output.stdout.len()
                );
                assert!(
                    output.stdout.contains(&b'F'),
                    "flood helper must actually run; libtest-only stdout={:?}",
                    String::from_utf8_lossy(&output.stdout)
                );
            }
            Err(error) => assert_eq!(
                error,
                BoundedChildError::Timeout,
                "flood must end as cap-bounded output or the shared deadline"
            ),
        }
    }

    #[test]
    fn parent_exit_with_descendant_holding_the_pipe_times_out() {
        let control = run_under_guard(
            helper_command("ok"),
            Duration::from_secs(2),
            1024,
            Duration::from_secs(4),
        )
        .expect("the same helper with no descendant must reach EOF");
        assert_eq!(control.exit_code, 0);
        assert!(
            control.stdout.windows(3).any(|w| w == b"ok\n"),
            "ok control must run the helper, not an empty libtest filter: {:?}",
            String::from_utf8_lossy(&control.stdout)
        );

        let error = run_under_guard(
            helper_command("hold-pipe"),
            Duration::from_secs(2),
            1024,
            Duration::from_secs(4),
        )
        .expect_err(
            "joining or read_capped after parent exit hangs while the descendant holds the pipe",
        );
        assert_eq!(error, BoundedChildError::Timeout);
    }
}
