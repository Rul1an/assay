//! Experimental capture only. No ID selection, policy evaluation or strong-key admission.
use crate::mcp::decision::{DecisionEmitter, DecisionEvent};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Write},
    sync::{Arc, Mutex},
};

const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RECORDS: u64 = 48;
const MAX_TOTAL: usize = 4 * 1024 * 1024;

pub(super) struct Sink {
    file: File,
    scope: String,
    sequence: u64,
    bytes: usize,
    stopped: bool,
}
impl Sink {
    pub(super) fn new(file: File) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            file,
            scope: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            bytes: 0,
            stopped: false,
        }))
    }
    fn stop(&mut self) {
        if !self.stopped {
            eprintln!("[assay] request receipt capture incomplete");
        }
        self.stopped = true;
    }
}

struct RequestEmitter {
    inner: Arc<dyn DecisionEmitter>,
    sink: Arc<Mutex<Sink>>,
    line: String,
    rpc_id_present: bool,
    rpc_id: Value,
}

pub(super) fn for_request(
    inner: Arc<dyn DecisionEmitter>,
    sink: Arc<Mutex<Sink>>,
    line: &str,
    rpc_id_present: bool,
    rpc_id: &Value,
) -> Arc<dyn DecisionEmitter> {
    let Ok(mut guard) = sink.lock() else {
        eprintln!("[assay] request receipt capture incomplete");
        return inner;
    };
    if guard.stopped {
        return inner;
    }
    if line.len() > MAX_REQUEST || guard.sequence >= MAX_RECORDS {
        guard.stop();
        return inner;
    }
    drop(guard);
    Arc::new(RequestEmitter {
        inner,
        sink,
        line: line.to_owned(),
        rpc_id_present,
        rpc_id: rpc_id.clone(),
    })
}

/// Stop serialization before it grows beyond the remaining capture budget.
struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("receipt limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn ndjson(value: &impl Serialize, limit: usize) -> io::Result<Vec<u8>> {
    let mut out = LimitedBytes {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    Ok(out.bytes)
}

#[derive(Serialize)]
struct Receipt<'a> {
    schema: &'static str,
    assertion_basis: &'static str,
    scope: &'a str,
    sequence: u64,
    decision_event_id: &'a str,
    decision_sha256: String,
    request_line: &'a str,
    request_bytes: usize,
    request_sha256: String,
    rpc_id_present: bool,
    rpc_id: &'a Value,
    metadata_pointer: &'static str,
}
impl DecisionEmitter for RequestEmitter {
    fn emit(&self, event: &DecisionEvent) {
        // The very same event reaches the original emitter. Receipt is not a log durability claim.
        self.inner.emit(event);
        let Ok(mut sink) = self.sink.lock() else {
            eprintln!("[assay] request receipt capture incomplete");
            return;
        };
        if sink.stopped {
            return;
        }
        if sink.sequence >= MAX_RECORDS {
            sink.stop();
            return;
        }
        let result = (|| -> io::Result<()> {
            let decision_bytes = ndjson(event, MAX_REQUEST)?;
            let receipt = Receipt {
                schema: "assay.request_decision_receipt.v0",
                assertion_basis: "caller_asserted",
                scope: &sink.scope,
                sequence: sink.sequence,
                decision_event_id: &event.id,
                decision_sha256: hex::encode(Sha256::digest(&decision_bytes)),
                request_line: &self.line,
                request_bytes: self.line.len(),
                request_sha256: hex::encode(Sha256::digest(self.line.as_bytes())),
                rpc_id_present: self.rpc_id_present,
                rpc_id: &self.rpc_id,
                metadata_pointer: "/params/_meta",
            };
            let bytes = ndjson(&receipt, MAX_TOTAL.saturating_sub(sink.bytes))?;
            sink.file.write_all(&bytes)?;
            sink.file.flush()?;
            sink.bytes += bytes.len();
            sink.sequence += 1;
            Ok(())
        })();
        if result.is_err() {
            sink.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl DecisionEmitter for Counter {
        fn emit(&self, _: &DecisionEvent) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn event() -> DecisionEvent {
        DecisionEvent::new("assay://test".into(), "req_2".into(), "echo".into())
    }
    #[test]
    fn total_budget_and_serialization_limit_stop_without_suppressing_decision() {
        let sink = Sink::new(tempfile::tempfile().unwrap());
        sink.lock().unwrap().bytes = MAX_TOTAL - 1;
        let inner = Arc::new(Counter::default());
        let emitter = for_request(inner.clone(), sink.clone(), "{}\n", true, &Value::from(2));
        emitter.emit(&event());
        assert_eq!(inner.0.load(Ordering::SeqCst), 1);
        assert!(sink.lock().unwrap().stopped);
        assert_eq!(sink.lock().unwrap().sequence, 0);
        assert!(ndjson(&"longer than budget", 3).is_err());
    }
    #[test]
    fn repeated_emission_cannot_bypass_record_limit() {
        let sink = Sink::new(tempfile::tempfile().unwrap());
        let inner = Arc::new(Counter::default());
        let emitter = for_request(inner.clone(), sink.clone(), "{}\n", true, &Value::from(2));
        for _ in 0..=MAX_RECORDS {
            emitter.emit(&event());
        }
        assert_eq!(inner.0.load(Ordering::SeqCst), MAX_RECORDS as usize + 1);
        assert_eq!(sink.lock().unwrap().sequence, MAX_RECORDS);
        assert!(sink.lock().unwrap().stopped);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn sink_failure_preserves_inner_emission_and_stops_capture() {
        let sink = Sink::new(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .unwrap(),
        );
        let inner = Arc::new(Counter::default());
        for_request(inner.clone(), sink.clone(), "{}\n", true, &Value::from(2)).emit(&event());
        assert_eq!(inner.0.load(Ordering::SeqCst), 1);
        assert!(sink.lock().unwrap().stopped);
        for_request(inner.clone(), sink.clone(), "{}\n", true, &Value::from(2)).emit(&event());
        assert_eq!(inner.0.load(Ordering::SeqCst), 2);
        assert_eq!(sink.lock().unwrap().sequence, 0);
    }
}
