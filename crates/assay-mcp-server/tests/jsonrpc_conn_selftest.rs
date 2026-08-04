//! Self-tests for the shared reader in `tests/jsonrpc_conn/`.
//!
//! The reader is included by eight test crates in this directory, so a `#[test]` inside the module
//! itself would run eight times. It lives here instead, in the one crate whose job is to check it.

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(1);
/// Generous headroom over `BUDGET`: this asserts the deadline is enforced at all, not that it is
/// precise, so a loaded machine does not turn a real bound into a flaky one.
const PATIENCE: Duration = Duration::from_secs(5);

/// A producer that outruns the consumer, which is what makes these regression tests.
///
/// `yes` and not a shell `while` loop on purpose: a shell loop produces more slowly than these
/// skip loops consume, so the channel drains, the receive finds it empty, `recv_timeout` reports
/// `Timeout`, and the run bounds itself even with the deadline check removed. Verified — the first
/// version of this test used `sh -c 'while :; do echo; done'` and passed against the unfixed
/// reader, which means it was testing the silent-child path and not this one.
#[cfg(unix)]
fn flood(line: &str) -> Child {
    Command::new("yes")
        .arg(line)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn `yes` (flooding producer)")
}

/// Run one read against a flooding child and return how long it took to give up.
///
/// The read fails by panicking, so it runs on its own thread: a panic on the test thread would
/// abort before the elapsed time could be reported, and a read that is NOT bounded would hang the
/// test rather than fail it with a usable message.
#[cfg(unix)]
fn time_until_the_read_gives_up(mut conn: Conn, read: fn(&mut Conn)) -> Duration {
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    let reader = std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&mut conn)));
        // `conn` is dropped here, which kills the flooding child.
        let _ = tx.send(outcome.is_err());
    });

    let Ok(panicked) = rx.recv_timeout(PATIENCE) else {
        panic!(
            "the read ran {PATIENCE:?} past a {BUDGET:?} budget: the deadline does not bound a \
             child that writes skippable lines faster than they are skipped"
        );
    };
    assert!(
        panicked,
        "the read returned a value; a flood of skippable lines is never a response"
    );
    reader.join().expect("reader thread");
    started.elapsed()
}

/// `read_json` skips blank lines. A child that writes them faster than they are skipped must not
/// be able to outrun the deadline.
///
/// The silent-child case and this one fail differently: there, no line ever arrives and
/// `recv_timeout` reports `Timeout`; here the channel is never empty when the receive is
/// attempted, and `recv_timeout`'s optimistic `try_recv` hands back a queued line even at zero
/// remaining duration. Only the explicit deadline check in `next_line` ends this run.
#[cfg(unix)]
#[test]
fn a_flood_of_blank_lines_cannot_outrun_the_deadline() {
    let conn = Conn::attach(flood("")).with_startup_timeout(BUDGET);
    let elapsed = time_until_the_read_gives_up(conn, |c| {
        c.read_json();
    });
    assert!(
        elapsed < PATIENCE,
        "the deadline did not bound the read: gave up only after {elapsed:?} (budget was {BUDGET:?})"
    );
}

/// The same for `read_response`, whose skip loop is the one the ~85 call sites in this directory
/// actually use: it discards notifications and upstream-initiated requests, so an upstream
/// emitting them in a loop is the realistic shape of this defect.
#[cfg(unix)]
#[test]
fn a_flood_of_notifications_cannot_outrun_the_deadline() {
    let notification = r#"{"jsonrpc":"2.0","method":"notifications/message"}"#;
    let conn = Conn::attach(flood(notification)).with_startup_timeout(BUDGET);
    let elapsed = time_until_the_read_gives_up(conn, |c| {
        c.read_response();
    });
    assert!(
        elapsed < PATIENCE,
        "the deadline did not bound the read: gave up only after {elapsed:?} (budget was {BUDGET:?})"
    );
}
