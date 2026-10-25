use super::event_types::DecisionEvent;
use std::io::Write;

/// Trait for emitting decision events.
pub trait DecisionEmitter: Send + Sync {
    /// Emit a decision event.
    fn emit(&self, event: &DecisionEvent);
}

/// File-based decision emitter (NDJSON).
pub struct FileDecisionEmitter {
    file: std::sync::Mutex<Box<dyn Write + Send>>,
}

impl FileDecisionEmitter {
    /// Create a new file emitter.
    pub fn new(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self::from_file(file))
    }

    /// Write to a file the caller already opened, for example one it created exclusively.
    pub fn from_file(file: std::fs::File) -> Self {
        Self::from_writer(Box::new(file))
    }

    fn from_writer(out: Box<dyn Write + Send>) -> Self {
        Self {
            file: std::sync::Mutex::new(out),
        }
    }

    #[cfg(test)]
    fn incomplete(&self) -> bool {
        false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::decision::reason_codes;
    use std::sync::{Arc, Mutex};

    /// Fails any write whose bytes contain `fail_on` and records the rest.
    struct Flaky {
        fail_on: Option<&'static str>,
        written: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Flaky {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(marker) = self.fail_on {
                if String::from_utf8_lossy(buf).contains(marker) {
                    return Err(std::io::Error::other("injected write failure"));
                }
            }
            self.written.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn event(id: &str) -> DecisionEvent {
        DecisionEvent::new(
            "assay://test".to_string(),
            id.to_string(),
            "test_tool".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
    }

    fn emitter(fail_on: Option<&'static str>) -> (FileDecisionEmitter, Arc<Mutex<Vec<u8>>>) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let out = Flaky {
            fail_on,
            written: written.clone(),
        };
        (FileDecisionEmitter::from_writer(Box::new(out)), written)
    }

    fn lines(written: &Arc<Mutex<Vec<u8>>>) -> Vec<String> {
        String::from_utf8(written.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_event_is_one_line_when_writes_succeed() {
        let (e, written) = emitter(None);
        e.emit(&event("tc_1"));
        e.emit(&event("tc_2"));
        let lines = lines(&written);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("tc_1") && lines[1].contains("tc_2"));
    }

    #[test]
    fn a_failed_write_ends_the_log_instead_of_leaving_a_gap() {
        // The second event's write fails; the third would succeed. Writing the third would
        // produce a log that looks contiguous but lacks an event.
        let (e, written) = emitter(Some("tc_2"));
        e.emit(&event("tc_1"));
        e.emit(&event("tc_2"));
        e.emit(&event("tc_3"));
        let lines = lines(&written);
        assert_eq!(lines.len(), 1, "log must stop at the failure: {lines:?}");
        assert!(lines[0].contains("tc_1"));
        assert!(
            e.incomplete(),
            "a failed write must mark the log incomplete"
        );
    }
}
