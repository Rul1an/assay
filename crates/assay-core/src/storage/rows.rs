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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRow {
    pub id: i64,
    pub step_id: String,
    pub episode_id: String,
    pub tool_name: Option<String>,
    pub call_index: Option<i32>,
    pub args: Option<String>,
    pub result: Option<String>,
}
