//! Progress reporting for run progress (E4.3). Used by the runner to emit done/total
//! in completion order; console layer consumes via a sink.

use std::sync::Arc;

/// One progress update: how many tests are done and total count.
#[derive(Debug, Clone, Copy)]
pub struct ProgressEvent {
    pub done: usize,
    pub total: usize,
}

/// Sink for progress events. Runner calls this each time a test completes.
/// Implementations may throttle (e.g. max N updates/sec or every k tests).
pub type ProgressSink = Arc<dyn Fn(ProgressEvent) + Send + Sync>;
