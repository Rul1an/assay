use super::event_types::DecisionEvent;
use std::io::Write;

/// Trait for emitting decision events.
pub trait DecisionEmitter: Send + Sync {
    /// Emit a decision event.
    fn emit(&self, event: &DecisionEvent);
}

/// File-based decision emitter (NDJSON).
pub struct FileDecisionEmitter {
    file: std::sync::Mutex<std::fs::File>,
}

impl FileDecisionEmitter {
    /// Create a new file emitter.
    pub fn new(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: std::sync::Mutex::new(file),
        })
    }
}

impl DecisionEmitter for FileDecisionEmitter {
    fn emit(&self, event: &DecisionEvent) {
        if let Ok(json) = serde_json::to_string(event) {
            if let Ok(mut f) = self.file.lock() {
                let _ = writeln!(f, "{}", json);
            }
        }
    }
}

/// Null emitter for testing.
pub struct NullDecisionEmitter;

impl DecisionEmitter for NullDecisionEmitter {
    fn emit(&self, _event: &DecisionEvent) {}
}
