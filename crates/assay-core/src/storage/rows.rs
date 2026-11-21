use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeRow {
    pub id: String,
    pub run_id: Option<i64>,
    pub test_id: Option<String>,
    pub timestamp: i64,
    pub prompt: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRow {
    pub id: String,
    pub episode_id: String,
    pub idx: i32,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
}

assay_common::closed! {
    #[derive(Serialize)]
    pub struct ToolCallRow {
        id: i64,
        step_id: String,
        episode_id: String,
        tool_name: Option<String>,
        call_index: Option<i32>,
        args: Option<String>,
        result: Option<String>,
    }
}
