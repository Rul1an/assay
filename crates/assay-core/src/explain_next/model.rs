use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step in the explained trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainedStep {
    pub index: usize,
    pub tool: String,

    /// Tool arguments (if available)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,

    /// Verdict for this step
    pub verdict: StepVerdict,

    /// Rules that were evaluated
    pub rules_evaluated: Vec<RuleEvaluation>,

    /// Current state of stateful rules after this step
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_snapshot: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepVerdict {
    /// Tool call allowed
    Allowed,
    /// Tool call blocked by a rule
    Blocked,
    /// Tool call allowed but triggered a warning
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub rule_id: String,
    pub rule_type: String,
    pub passed: bool,
    pub explanation: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Complete explanation of a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceExplanation {
    pub policy_name: String,
    pub policy_version: String,
    pub total_steps: usize,
    pub allowed_steps: usize,
    pub blocked_steps: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_block_index: Option<usize>,

    /// Detailed step-by-step explanation
    pub steps: Vec<ExplainedStep>,

    /// Summary of rules that blocked
    pub blocking_rules: Vec<String>,
}

/// Tool call input for explanation
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    /// Tool name
    #[serde(alias = "name", alias = "tool_name")]
    pub tool: String,

    /// Tool arguments
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}
