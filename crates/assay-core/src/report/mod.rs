pub mod console;
pub mod json;
pub mod junit;
pub mod sarif;
pub mod summary;

use crate::model::TestResultRow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArtifacts {
    pub run_id: i64,
    pub suite: String,
    pub results: Vec<TestResultRow>,
}
