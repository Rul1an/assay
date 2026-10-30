use super::event_types::DecisionEvent;
use sha2::{Digest, Sha256};
use std::{io::Write, sync::Mutex};

/// Trait for emitting decision events.
pub trait DecisionEmitter: Send + Sync {
    /// Emit a decision event.
    fn emit(&self, event: &DecisionEvent);
}

/// Per-emitter accounting. Final only after every emitting thread has joined.
/// A completed write is not an fsync or power-loss guarantee.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DecisionLogStatus {
    pub written: u64,
    pub dropped: u64,
    pub incomplete: bool,
    /// SHA-256 of complete successfully written lines, excluding a partial failed tail.
    pub prefix_sha256: String,
}

struct WriterState {
    file: Box<dyn Write + Send>,
    stopped: bool,
    written: u64,
    dropped: u64,
    digest: Sha256,
}

impl WriterState {
    fn stop(&mut self, cause: &str) {
        self.dropped += 1;
        if !self.stopped {
            self.stopped = true;
            eprintln!(
                "[assay] decision log incomplete: {cause}; no further decision events are written"
            );
        }
    }
}

/// NDJSON writer: a failed write ends the stream; subsequent events are counted
/// but never appended. A failure may leave a partial final line.
pub struct FileDecisionEmitter {
    file: Mutex<WriterState>,
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

    fn from_writer(file: Box<dyn Write + Send>) -> Self {
        Self {
            file: Mutex::new(WriterState {
                file,
                stopped: false,
                written: 0,
                dropped: 0,
                digest: Sha256::new(),
            }),
        }
    }

    /// Consistent snapshot; final only after all emitting threads have joined.
    pub fn status(&self) -> DecisionLogStatus {
        let (state, poisoned) = match self.file.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        DecisionLogStatus {
            written: state.written,
            dropped: state.dropped,
            incomplete: state.stopped || poisoned,
            prefix_sha256: hex::encode(state.digest.clone().finalize()),
        }
    }

    #[cfg(test)]
    fn incomplete(&self) -> bool {
        self.status().incomplete
    }
}

impl DecisionEmitter for FileDecisionEmitter {
    fn emit(&self, event: &DecisionEvent) {
        let mut state = match self.file.lock() {
            Ok(state) => state,
            Err(error) => {
                error.into_inner().stop("the log lock was poisoned");
                return;
            }
        };
        if state.stopped {
            state.dropped += 1;
            return;
        }
        let mut line = match serde_json::to_vec(event) {
            Ok(json) => json,
            Err(_) => {
                state.stop("an event could not be serialized");
                return;
            }
        };
        line.push(b'\n');
        match state.file.write_all(&line) {
            Ok(()) => {
                state.digest.update(&line);
                state.written += 1;
            }
            Err(error) => state.stop(&format!("write failed ({})", error.kind())),
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
    fn status_counts_failed_event_and_every_later_refusal() {
        let (e, _) = emitter(Some("tc_2"));
        e.emit(&event("tc_1"));
        e.emit(&event("tc_2"));
        e.emit(&event("tc_3"));
        let s = e.status();
        assert_eq!(s.written, 1);
        assert_eq!(s.dropped, 2);
        assert!(s.incomplete);
    }

    #[test]
    fn concurrent_events_have_one_consistent_byte_digest() {
        let (e, written) = emitter(None);
        let e = Arc::new(e);
        let workers: Vec<_> = (0..8)
            .map(|n| {
                let e = e.clone();
                std::thread::spawn(move || e.emit(&event(&format!("tc_{n}"))))
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let status = e.status();
        assert_eq!(status.written, 8);
        assert_eq!(status.dropped, 0);
        assert!(!status.incomplete);
        assert_eq!(
            status.prefix_sha256,
            hex::encode(Sha256::digest(&*written.lock().unwrap()))
        );
    }

    #[test]
    fn partial_write_is_not_counted_or_hashed_as_a_complete_event() {
        struct Partial {
            first: bool,
        }
        impl Write for Partial {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                if self.first {
                    self.first = false;
                    Ok(1)
                } else {
                    Err(std::io::Error::other("partial write"))
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let e = FileDecisionEmitter::from_writer(Box::new(Partial { first: true }));
        e.emit(&event("partial"));
        e.emit(&event("later"));
        let status = e.status();
        assert_eq!(status.written, 0);
        assert_eq!(status.dropped, 2);
        assert!(status.incomplete);
        assert_eq!(status.prefix_sha256, hex::encode(Sha256::digest([])));
    }

    #[test]
    fn poisoned_writer_is_never_reported_complete() {
        let (e, _) = emitter(None);
        let e = Arc::new(e);
        let other = e.clone();
        let _ = std::thread::spawn(move || {
            let _lock = other.file.lock().unwrap();
            panic!("injected");
        })
        .join();
        assert!(e.status().incomplete);
        e.emit(&event("later"));
        assert_eq!(e.status().dropped, 1);
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
