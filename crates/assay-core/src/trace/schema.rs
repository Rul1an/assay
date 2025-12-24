use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraceEntry {
    V1(TraceEntryV1),
    V2(TraceEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntryV1 {
    pub schema_version: u32,
    #[serde(rename = "type")]
    pub entry_type: String, // "assay.trace"
    pub request_id: String,
    pub prompt: String,
    pub response: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}

// --- V2 Schema (Streamable) ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TraceEvent {
    #[serde(rename = "episode_start")]
    EpisodeStart(EpisodeStart),
    #[serde(rename = "step")]
    Step(StepEntry),
    #[serde(rename = "tool_call")]
    ToolCall(ToolCallEntry),
    #[serde(rename = "episode_end")]
    EpisodeEnd(EpisodeEnd),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeStart {
    pub episode_id: String,
    pub timestamp: u64, // ms
    #[serde(default)]
    pub input: serde_json::Value, // { prompt: ... }
    #[serde(default)]
    pub meta: serde_json::Value,
}

// --- Provenance ---

// --- Provenance ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruncationMeta {
    pub field: String,
    pub original_len: usize,
    pub kept_len: usize,
    pub sha256: String,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepEntry {
    pub episode_id: String,
    pub step_id: String,
    #[serde(default)]
    pub idx: u32,
    pub timestamp: u64,
    pub kind: String,
    pub name: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>, // Hash of full content
    #[serde(default)]
    pub truncations: Vec<TruncationMeta>,
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallEntry {
    pub episode_id: String,
    pub step_id: String,
    pub timestamp: u64,
    pub tool_name: String,
    pub call_index: Option<u32>, // Added for uniqueness (step_id, call_index)
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub args_sha256: Option<String>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub result_sha256: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub truncations: Vec<TruncationMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeEnd {
    pub episode_id: String,
    pub timestamp: u64,
    pub outcome: Option<String>, // "pass", "fail", "error"
    #[serde(default)]
    pub final_output: Option<String>,
}
