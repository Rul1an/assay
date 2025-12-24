use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceAssertion {
    #[serde(rename = "trace_must_call_tool")]
    TraceMustCallTool {
        tool: String,
        min_calls: Option<u32>,
    },
    #[serde(rename = "trace_must_not_call_tool")]
    TraceMustNotCallTool { tool: String },
    #[serde(rename = "trace_tool_sequence")]
    TraceToolSequence {
        sequence: Vec<String>,
        allow_other_tools: bool,
    },
    #[serde(rename = "trace_max_steps")]
    TraceMaxSteps { max: u32 },
}
