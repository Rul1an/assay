//! Timeout-bounded JSON-RPC-over-stdio plumbing shared by the e2e tests in this directory.
//!
//! Every one of those tests drives a spawned proxy or server over newline-delimited JSON-RPC on
//! stdin/stdout. Read with a bare [`BufRead::read_line`], a child that spawns but never answers
//! wedges the test forever: `read_line` blocks inside the syscall, so no elapsed-time check in the
//! surrounding loop is ever reached. Under `cargo nextest` that surfaces as the `.config/nextest.toml`
//! `slow-timeout = { period = "60s", terminate-after = 2 }` SIGTERM at 120s with no indication of
//! which request went unanswered, and `retries = 1` then buys a second 120s hang.
//!
//! [`Conn`] moves the blocking read onto a worker thread and waits on it with
//! [`std::sync::mpsc::Receiver::recv_timeout`], which is cancellable. On timeout it kills the child
//! rather than leaving a wedged process orphaned onto the inherited stderr, and it names the request
//! it was waiting for — which it can only do because sends go through the same handle as reads.

// Each test crate in this directory uses a different subset of this module.
#![allow(dead_code)]

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ExitStatus};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long one exchange may take before the test fails instead of hanging.
///
/// Deliberately well under nextest's 120s terminate-after kill, so a wedged child produces this
/// module's message rather than an unexplained SIGTERM.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the child may take to exit once it has seen EOF on stdin.
const REAP_TIMEOUT: Duration = Duration::from_secs(10);

/// How many lines the reader thread may run ahead of the test before it blocks.
///
/// The channel is bounded because the deadline bounds time, not memory. A child writing lines
/// faster than the caller skips them fills an unbounded channel for the whole budget: measured at
/// ~110 MB/s against `yes`, which is ~3.3 GB at [`DEFAULT_TIMEOUT`] and grows with any longer
/// budget a caller sets through [`Conn::with_startup_timeout`]. With a bound the reader thread
/// stops draining stdout, the child blocks on its own pipe, and the test keeps control of the
/// deadline.
///
/// The bound is large rather than minimal on purpose. Backpressure introduces a deadlock the
/// unbounded channel could not have: a test that writes several requests before reading any could,
/// with a tiny bound, block in `send` on a full stdin pipe while the child blocks on a full stdout
/// pipe — and `send` is not covered by the read deadline. 1024 lines plus the child's own ~64 KB
/// stdout pipe is far beyond any interleaving in this directory (the deepest is two unread sends),
/// while still costing only tens of kilobytes.
const READ_AHEAD_LINES: usize = 1024;

/// Cap on what [`Conn::drain_stdout_after_shutdown`] will collect, for the same reason: the lines
/// it returns are accumulated in memory, so a still-running child would trade an unbounded wait for
/// unbounded memory.
const MAX_DRAINED_LINES: usize = 10_000;

/// What the reader thread hands back: a line, or the error that ended the stream.
enum Chunk {
    Line(String),
    Err(String),
}

/// A live stdio JSON-RPC connection to a spawned child, with every read bounded by a deadline.
pub struct Conn {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Chunk>,
    timeout: Duration,
    /// Budget for the first read only; startup is slow in a way steady state is not.
    startup_timeout: Option<Duration>,
    /// The budget the in-flight read is using, so a failure reports the deadline it actually had.
    current_budget: Duration,
    /// The most recent request written, so a timeout can name the exchange that stalled.
    last_sent: Option<String>,
    responses_read: usize,
}

impl Conn {
    /// Take over a spawned child's stdin/stdout and start the reader thread.
    ///
    /// The child must have been spawned with both piped.
    pub fn attach(mut child: Child) -> Self {
        let stdin = child
            .stdin
            .take()
            .expect("child must be spawned with stdin piped");
        let stdout = child
            .stdout
            .take()
            .expect("child must be spawned with stdout piped");
        let (tx, rx) = mpsc::sync_channel(READ_AHEAD_LINES);

        // Detached on purpose: joining this thread is exactly the block this module exists to
        // avoid. It ends on its own when the child's stdout closes, which killing the child forces,
        // or when a blocked `send` fails because the test dropped its `Conn`.
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    // EOF. Dropping `tx` disconnects the channel, which the reader reports as EOF.
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(Chunk::Line(line)).is_err() {
                            break; // the test dropped its Conn
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Chunk::Err(e.to_string()));
                        break;
                    }
                }
            }
        });

        Self {
            child,
            stdin: Some(stdin),
            rx,
            timeout: DEFAULT_TIMEOUT,
            startup_timeout: None,
            current_budget: DEFAULT_TIMEOUT,
            last_sent: None,
            responses_read: 0,
        }
    }

    /// Give the FIRST read a longer budget than the rest.
    ///
    /// Only startup is slow, so only startup gets the allowance. Extending it to every read would
    /// let two slow exchanges add up past the harness's own kill, which is the failure this module
    /// exists to keep off the table. Whatever is passed must therefore leave room for one further
    /// [`DEFAULT_TIMEOUT`] under nextest's 120s `terminate-after`.
    ///
    /// No caller needs this today: every test here spawns a prebuilt `CARGO_BIN_EXE_*` binary, so
    /// no first exchange waits on a compile. It is kept because the constraint above is the part
    /// that is easy to get wrong, and it is written down here rather than rediscovered.
    #[must_use]
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = Some(timeout);
        self
    }

    /// The budget for the read that is about to start, consuming the startup allowance if unused.
    fn take_budget(&mut self) -> Duration {
        let budget = self.startup_timeout.take().unwrap_or(self.timeout);
        self.current_budget = budget;
        budget
    }

    /// Write one request as a JSON line, remembering it so a later timeout can name it.
    pub fn send(&mut self, v: Value) {
        self.last_sent = Some(describe(&v));
        let stdin = self.stdin.as_mut().expect("stdin is still open");
        writeln!(stdin, "{v}").expect("write request");
        stdin.flush().expect("flush request");
    }

    /// Send a request and read the response to it.
    ///
    /// Reads with [`Conn::read_response`] and not [`Conn::read_json`]: the name promises the
    /// response, so a notification arriving first must not be handed back in its place.
    pub fn request(&mut self, method: &str, params: Value, id: u64) -> Value {
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id
        }));
        self.read_response()
    }

    /// The next non-blank stdout line, parsed as JSON.
    pub fn read_json(&mut self) -> Value {
        let deadline = Instant::now() + self.take_budget();
        self.read_json_by(deadline)
    }

    /// [`Conn::read_json`], but skipping notifications and upstream-initiated requests, which carry
    /// a `method` and are not the response this caller is waiting for.
    ///
    /// The deadline spans the whole skip loop, so a child that only emits notifications still fails
    /// on time instead of renewing its budget with every one.
    pub fn read_response(&mut self) -> Value {
        let deadline = Instant::now() + self.take_budget();
        loop {
            let v = self.read_json_by(deadline);
            if v.get("method").is_none() {
                return v;
            }
        }
    }

    /// [`Conn::read_response`], but also skips responses for other numeric request IDs.
    pub fn read_response_for_id(&mut self, expected_id: u64) -> Value {
        let deadline = Instant::now() + self.take_budget();
        loop {
            let v = self.read_json_by(deadline);
            if v.get("method").is_none() && v.get("id").and_then(Value::as_u64) == Some(expected_id)
            {
                return v;
            }
        }
    }

    /// Every remaining stdout line up to EOF, used to prove nothing further reached the client.
    ///
    /// Named for its precondition because it has one: the returned lines are accumulated in memory,
    /// so this is only bounded when EOF is actually coming. On a still-running child there is no
    /// EOF to wait for, and it stops at [`MAX_DRAINED_LINES`] or at the deadline, whichever first.
    pub fn drain_stdout_after_shutdown(&mut self) -> Vec<String> {
        let deadline = Instant::now() + self.take_budget();
        let mut lines = Vec::new();
        loop {
            // Same reason as in `next_line`: a queued line comes back even at zero remaining.
            let now = Instant::now();
            if now >= deadline {
                self.fail("the child held stdout open past the deadline while draining it");
            }
            if lines.len() >= MAX_DRAINED_LINES {
                self.fail(&format!(
                    "the child wrote more than {MAX_DRAINED_LINES} lines after shutdown; \
                     draining it is only bounded when EOF is actually coming"
                ));
            }
            let remaining = deadline.saturating_duration_since(now);
            match self.rx.recv_timeout(remaining) {
                Ok(Chunk::Line(line)) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }
                // EOF is the expected end. A read error is not, and must not look like one.
                Err(RecvTimeoutError::Disconnected) => return lines,
                Ok(Chunk::Err(e)) => self.fail(&format!(
                    "reading the child's stdout failed while draining: {e}"
                )),
                Err(RecvTimeoutError::Timeout) => {
                    self.fail("the child held stdout open past the deadline while draining it")
                }
            }
        }
    }

    /// Close stdin, signalling client EOF.
    pub fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }

    /// Close stdin and reap the child.
    pub fn shutdown(&mut self) -> ExitStatus {
        self.close_stdin();
        self.wait()
    }

    /// Wait for the child to exit, killing it if it overruns [`REAP_TIMEOUT`].
    ///
    /// A child that never exits after stdin EOF hangs the test exactly like a child that never
    /// answers, so this is bounded for the same reason the reads are.
    pub fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + REAP_TIMEOUT;
        loop {
            match self.child.try_wait().expect("try_wait on child") {
                Some(status) => return status,
                None if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    let status = self.child.wait().expect("wait after kill");
                    panic!(
                        "the child did not exit within {REAP_TIMEOUT:?} of stdin EOF; killed it \
                         ({status}). Last request sent: {}",
                        self.last_request()
                    );
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    /// Kill the child and reap it.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// The next non-blank line parsed as JSON, bounded by an externally owned deadline.
    fn read_json_by(&mut self, deadline: Instant) -> Value {
        loop {
            let line = self.next_line(deadline);
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            return match serde_json::from_str(trimmed) {
                Ok(v) => {
                    self.responses_read += 1;
                    v
                }
                Err(e) => self.fail(&format!("stdout line is not JSON ({e}): {trimmed}")),
            };
        }
    }

    /// The next line from the reader thread, or a failure that names the stalled exchange.
    ///
    /// The deadline is checked here explicitly rather than left to `recv_timeout`. `recv_timeout`
    /// attempts an optimistic `try_recv` first, so it hands back an already-queued line even when
    /// the remaining duration is zero; a child writing skippable lines faster than the callers
    /// above skip them therefore keeps the channel non-empty and would run unbounded past the
    /// deadline. `jsonrpc_conn_selftest.rs` covers that case, one test per skip loop:
    /// `a_flood_of_blank_lines_cannot_outrun_the_deadline` and
    /// `a_flood_of_notifications_cannot_outrun_the_deadline`.
    ///
    /// Credit: the same defect was found and fixed for the CLI's copy of this reader in #1987.
    fn next_line(&mut self, deadline: Instant) -> String {
        let now = Instant::now();
        if now >= deadline {
            self.fail("the deadline passed while skipping lines that were not the response");
        }
        let remaining = deadline.saturating_duration_since(now);
        match self.rx.recv_timeout(remaining) {
            Ok(Chunk::Line(line)) => line,
            Ok(Chunk::Err(e)) => self.fail(&format!("reading the child's stdout failed: {e}")),
            Err(RecvTimeoutError::Timeout) => {
                self.fail("the child wrote no complete line before the deadline")
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.fail("the child closed stdout (EOF) without answering")
            }
        }
    }

    /// Kill the child and fail the test, naming what the read was waiting for.
    ///
    /// Killing matters: these children inherit the harness's stderr, so one left running keeps
    /// writing into the output of whatever runs next.
    fn fail(&mut self, why: &str) -> ! {
        let _ = self.child.kill();
        let _ = self.child.wait();
        panic!(
            "{why}. Waiting for a response to {} ({} response(s) read on this connection, \
             timeout {:?}). The child has been killed.",
            self.last_request(),
            self.responses_read,
            self.current_budget
        );
    }

    fn last_request(&self) -> String {
        self.last_sent
            .clone()
            .unwrap_or_else(|| "<nothing: no request has been sent on this connection>".to_string())
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        // A test that fails for any other reason must not leave the child behind either.
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A short label for a request: enough to say which exchange stalled.
fn describe(v: &Value) -> String {
    let method = v
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("<no method>");
    match v.get("id") {
        Some(id) => format!("id={id} method={method}"),
        None => format!("notification method={method}"),
    }
}
