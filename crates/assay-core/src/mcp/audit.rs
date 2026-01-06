use serde::Serialize;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Serialize)]
pub struct AuditEvent {
    pub timestamp: String, // ISO 8601
    pub decision: String,  // "allow" | "deny" | "would_deny"
    pub tool: Option<String>,
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agentic: Option<Value>, // The contract payload
}

pub struct AuditLog {
    file: Option<std::fs::File>,
}

impl AuditLog {
    pub fn new(path: Option<&Path>) -> Self {
        let file = path.and_then(|p| OpenOptions::new().create(true).append(true).open(p).ok());
        Self { file }
    }

    pub fn log(&mut self, event: &AuditEvent) {
        if let Some(f) = &mut self.file {
            if let Ok(json) = serde_json::to_string(event) {
                writeln!(f, "{}", json).ok();
            }
        }
    }
}
