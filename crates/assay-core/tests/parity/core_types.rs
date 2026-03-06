use serde::{Deserialize, Serialize};

/// A single policy check to evaluate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheck {
    pub id: String,
    pub check_type: CheckType,
    pub params: serde_json::Value,
}

/// Types of policy checks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    ArgsValid,
    SequenceValid,
    ToolBlocklist,
}

/// Input to a policy check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInput {
    /// Tool being called (for args_valid, blocklist)
    pub tool_name: Option<String>,
    /// Arguments to validate
    pub args: Option<serde_json::Value>,
    /// Trace of tool calls (for sequence_valid)
    pub trace: Option<Vec<ToolCall>>,
}

/// A tool call in a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub timestamp_ms: u64,
}

/// Result of a policy check
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_id: String,
    pub outcome: Outcome,
    pub reason: String,
    /// Canonical hash for comparison
    pub result_hash: String,
}

/// Outcome of a check
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    Error,
}
