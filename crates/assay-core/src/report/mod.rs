pub mod console;
pub mod json;
pub mod junit;
pub mod progress;
pub mod sarif;
pub mod summary;

use crate::model::TestResultRow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArtifacts {
    pub run_id: i64,
    pub suite: String,
    pub results: Vec<TestResultRow>,
    /// Seed used for test order randomization (E7.2). Present when run used a seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_seed: Option<u64>,
    /// Estimated time spent cloning runner refs for test tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_clone_ms: Option<u64>,
    /// Number of clone operations used to produce per-task runner refs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_clone_count: Option<u64>,
}
