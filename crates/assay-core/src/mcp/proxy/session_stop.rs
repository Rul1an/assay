use super::McpProxy;
use crate::mcp::decision::DecisionLogStatus;
use std::{
    io,
    process::ExitStatus,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

/// Observations for one proxy invocation, not a whole-action or authenticity verdict.
#[derive(Debug)]
#[non_exhaustive]
pub struct ProxyRunReport {
    pub result: io::Result<i32>,
    pub server_pid: u32,
    pub child_status: Option<ExitStatus>,
    /// Assay attempted kill after try_wait did not return a status. This does not
    /// establish that the kill caused termination (the child can exit concurrently).
    pub assay_kill_requested: bool,
    pub decision_log: Option<DecisionLogStatus>,
    /// Every client frame was a `server/discover` request, and there was at least one. A client
    /// can probe on a disposable process before opening its session; this names that shape. It
    /// does not establish what the client intended.
    pub client_probe_only: bool,
}

/// What kinds of frame the client sent during one proxy invocation.
#[derive(Debug, Default)]
pub(super) struct ClientTraffic {
    discover: AtomicBool,
    other: AtomicBool,
}

impl ClientTraffic {
    /// Record one frame and say whether it was a probe. `frame` is `None` when the line did not
    /// parse: unreadable traffic is never a probe.
    pub(super) fn observe(&self, frame: Option<&serde_json::Value>) -> bool {
        let discover = frame.is_some_and(|value| {
            value.get("method").and_then(serde_json::Value::as_str) == Some("server/discover")
                && value.get("id").is_some_and(|id| !id.is_null())
        });
        let seen = if discover {
            &self.discover
        } else {
            &self.other
        };
        seen.store(true, Ordering::Relaxed);
        discover
    }

    pub(super) fn saw_discover(&self) -> bool {
        self.discover.load(Ordering::Relaxed)
    }

    pub(super) fn probe_only(&self) -> bool {
        self.discover.load(Ordering::Relaxed) && !self.other.load(Ordering::Relaxed)
    }
}

impl McpProxy {
    pub(super) fn finish_child(&mut self) {
        if self.child_status.is_some() || self.cleanup_attempted {
            return;
        }
        self.cleanup_attempted = true;
        if let Ok(Some(status)) = self.child.try_wait() {
            self.child_status = Some(status);
            return;
        }
        // Preserve the bounded best-effort cleanup from #3419. A missing status
        // after this deadline is unknown, never a fabricated exit or successful kill.
        self.kill_requested = true;
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.child_status = Some(status);
                    break;
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => break,
            }
        }
    }
}
