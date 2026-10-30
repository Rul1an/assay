use super::McpProxy;
use crate::mcp::decision::DecisionLogStatus;
use std::{
    io,
    process::ExitStatus,
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
