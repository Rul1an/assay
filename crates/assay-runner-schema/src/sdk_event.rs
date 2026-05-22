use serde::{Deserialize, Serialize};

pub const SDK_EVENT_SCHEMA: &str = "assay.runner.sdk_event.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkLayerEvent {
    pub schema: String,
    pub run_id: String,
    pub seq: u64,
    pub event_type: String,
    pub source: String,
    pub sdk_name: Option<String>,
    pub sdk_version: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool: Option<String>,
}
