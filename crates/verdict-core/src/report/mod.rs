pub mod console;
pub mod json;
pub mod junit;
pub mod sarif;

use crate::model::TestResultRow;

#[derive(Debug, Clone)]
pub struct RunArtifacts {
    pub run_id: i64,
    pub suite: String,
    pub results: Vec<TestResultRow>,
}
