use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntryV1 {
    pub schema_version: u32,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub request_id: String,
    pub prompt: String,
    pub response: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}
