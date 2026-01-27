pub mod engine;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BundleSummary {
    pub run_id: String,
    pub event_count: usize,
    pub run_root: String,
    pub time_range: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSet {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl DiffSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary {
    pub event_count_delta: i64,
    pub duration_delta: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub baseline: BundleSummary,
    pub candidate: BundleSummary,
    pub summary: DiffSummary,
    pub network: DiffSet,
    pub filesystem: DiffSet,
    pub processes: DiffSet,
}

impl DiffReport {
    pub fn is_empty(&self) -> bool {
        self.network.is_empty() && self.filesystem.is_empty() && self.processes.is_empty()
    }
}
