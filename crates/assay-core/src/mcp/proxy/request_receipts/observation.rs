//! Ask and answer observations share the receipts sidecar and its budget.
//! They are not decisions and not proof a human answered.

use super::Sink;
use crate::mcp::era::{observe_result, ResultObservation};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    sync::{Arc, Mutex},
};

const OBSERVATION_SCHEMA: &str = "assay.request_observation.v0";
const MAX_PENDING_CALLS: usize = 1024;

#[derive(Clone)]
enum CallLink {
    Unique(Value),
    Ambiguous,
    Unlinked,
}

struct RecordedAsk {
    rpc_id: Value,
    link: CallLink,
}

#[derive(Default)]
struct PendingCalls {
    ids: Vec<Value>,
}

pub(in super::super) struct Capture {
    sink: Mutex<Option<Arc<Mutex<Sink>>>>,
    pending: Mutex<PendingCalls>,
    asks: Mutex<Vec<RecordedAsk>>,
}

impl Capture {
    pub(in super::super) fn new(file: Option<File>) -> Arc<Self> {
        Arc::new(Self {
            sink: Mutex::new(file.map(Sink::new)),
            pending: Mutex::new(PendingCalls::default()),
            asks: Mutex::new(Vec::new()),
        })
    }

    pub(in super::super) fn attach(&self, file: File) {
        if let Ok(mut guard) = self.sink.lock() {
            *guard = Some(Sink::new(file));
        }
    }

    pub(in super::super) fn sink(&self) -> Option<Arc<Mutex<Sink>>> {
        self.sink.lock().ok().and_then(|guard| guard.clone())
    }

    pub(in super::super) fn forwarded_call(&self, id: &Value) {
        if self.sink().is_none() || id.is_null() {
            return;
        }
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        if pending.ids.len() >= MAX_PENDING_CALLS {
            pending.ids.clear();
        }
        pending.ids.push(id.clone());
    }

    pub(in super::super) fn observe_server_line(&self, line: &str) {
        let Some(sink) = self.sink() else {
            return;
        };
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            return;
        }
        let Ok(frame) = serde_json::from_str::<Value>(trimmed) else {
            return;
        };
        if !frame.is_object() {
            return;
        }
        let has_method = frame.get("method").is_some();
        let has_result = frame.get("result").is_some();
        let has_error = frame.get("error").is_some();
        let id = frame.get("id").cloned().filter(|id| !id.is_null());

        if has_method && !has_result && !has_error {
            if id.is_some() {
                let link = self.link_from_pending();
                self.record_ask(&sink, line, &frame, link);
            }
            return;
        }

        if let Some(id) = id.as_ref() {
            self.pending_remove(id);
        }

        if has_result && !has_error && is_input_required_ask(&frame) {
            let link = match id {
                Some(id) => CallLink::Unique(id),
                None => CallLink::Unlinked,
            };
            self.record_ask(&sink, line, &frame, link);
        }
    }

    pub(in super::super) fn observe_client_line(&self, line: &str, value: &Value) {
        let Some(sink) = self.sink() else {
            return;
        };
        if value.get("method").is_some() {
            return;
        }
        let Some(id) = value.get("id").cloned().filter(|id| !id.is_null()) else {
            return;
        };
        if value.get("result").is_none() && value.get("error").is_none() {
            return;
        }
        let link = self.link_for_answer(&id);
        write_observation(&sink, line, "answer", value.get("id").is_some(), &id, link);
    }

    fn link_from_pending(&self) -> CallLink {
        let Ok(pending) = self.pending.lock() else {
            return CallLink::Unlinked;
        };
        match pending.ids.as_slice() {
            [] => CallLink::Unlinked,
            [id] => CallLink::Unique(id.clone()),
            _ => CallLink::Ambiguous,
        }
    }

    fn pending_remove(&self, id: &Value) {
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        pending.ids.retain(|existing| existing != id);
    }

    fn record_ask(&self, sink: &Arc<Mutex<Sink>>, line: &str, frame: &Value, link: CallLink) {
        let rpc_id_present = frame.get("id").is_some();
        let rpc_id = frame.get("id").cloned().unwrap_or(Value::Null);
        if let Ok(mut asks) = self.asks.lock() {
            asks.push(RecordedAsk {
                rpc_id: rpc_id.clone(),
                link: link.clone(),
            });
        }
        write_observation(sink, line, "ask", rpc_id_present, &rpc_id, link);
    }

    fn link_for_answer(&self, id: &Value) -> CallLink {
        let Ok(asks) = self.asks.lock() else {
            return CallLink::Unlinked;
        };
        asks.iter()
            .find(|ask| ask.rpc_id == *id)
            .map(|ask| ask.link.clone())
            .unwrap_or(CallLink::Unlinked)
    }
}

fn is_input_required_ask(frame: &Value) -> bool {
    matches!(
        observe_result(frame),
        Some(
            ResultObservation::InputRequired
                | ResultObservation::InputRequiredWithMalformedContinuation
                | ResultObservation::InputRequiredWithoutContinuation
        )
    )
}

#[derive(Serialize)]
struct Observation<'a> {
    schema: &'static str,
    assertion_basis: &'static str,
    kind: &'static str,
    scope: &'a str,
    sequence: u64,
    request_line: &'a str,
    request_bytes: usize,
    request_sha256: String,
    rpc_id_present: bool,
    rpc_id: &'a Value,
    call_link: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    call_id: Option<&'a Value>,
}

fn write_observation(
    sink: &Arc<Mutex<Sink>>,
    line: &str,
    kind: &'static str,
    rpc_id_present: bool,
    rpc_id: &Value,
    link: CallLink,
) {
    let Ok(mut sink) = sink.lock() else {
        eprintln!("[assay] request receipt capture incomplete");
        return;
    };
    let (call_link, call_id) = match &link {
        CallLink::Unique(id) => ("unique", Some(id)),
        CallLink::Ambiguous => ("ambiguous", None),
        CallLink::Unlinked => ("unlinked", None),
    };
    let scope = sink.scope.clone();
    let sequence = sink.sequence;
    let record = Observation {
        schema: OBSERVATION_SCHEMA,
        assertion_basis: "observed_on_wire",
        kind,
        scope: &scope,
        sequence,
        request_line: line,
        request_bytes: line.len(),
        request_sha256: hex::encode(Sha256::digest(line.as_bytes())),
        rpc_id_present,
        rpc_id,
        call_link,
        call_id,
    };
    sink.write_json(line.len(), &record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn capture_file() -> (CaptureHandle, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let capture = Capture::new(Some(tmp.reopen().unwrap()));
        (CaptureHandle(capture), tmp)
    }

    struct CaptureHandle(Arc<Capture>);
    impl std::ops::Deref for CaptureHandle {
        type Target = Capture;
        fn deref(&self) -> &Capture {
            &self.0
        }
    }

    fn sidecar(tmp: &tempfile::NamedTempFile) -> Vec<Value> {
        let body = std::fs::read_to_string(tmp.path()).unwrap_or_default();
        body.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn oversize_ask_stops_capture_and_writes_nothing() {
        let (capture, tmp) = capture_file();
        capture.forwarded_call(&json!(4));
        let huge = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":"ask-1","method":"elicitation/create","params":{"message":"x".repeat(super::super::MAX_REQUEST)}})
        );
        capture.observe_server_line(&huge);
        assert!(capture.sink().unwrap().lock().unwrap().stopped);
        assert_eq!(capture.sink().unwrap().lock().unwrap().sequence, 0);
        assert!(sidecar(&tmp).is_empty());
    }

    #[test]
    fn two_pending_calls_do_not_guess_an_owner() {
        let (capture, tmp) = capture_file();
        capture.forwarded_call(&json!(4));
        capture.forwarded_call(&json!(5));
        let line = "{\"jsonrpc\":\"2.0\",\"id\":\"ask-1\",\"method\":\"elicitation/create\",\"params\":{}}\n";
        capture.observe_server_line(line);
        let records = sidecar(&tmp);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["call_link"], json!("ambiguous"));
        assert!(records[0].get("call_id").is_none());
        assert_eq!(records[0]["request_line"], json!(line));
    }

    #[test]
    fn sampling_after_completed_call_is_unlinked() {
        let (capture, tmp) = capture_file();
        capture.forwarded_call(&json!(4));
        capture.observe_server_line("{\"jsonrpc\":\"2.0\",\"id\":4,\"result\":{\"content\":[]}}\n");
        let line = "{\"jsonrpc\":\"2.0\",\"id\":\"samp-1\",\"method\":\"sampling/createMessage\",\"params\":{}}\n";
        capture.observe_server_line(line);
        let records = sidecar(&tmp);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["call_link"], json!("unlinked"));
        assert!(records[0].get("call_id").is_none());
        assert_ne!(records[0].get("call_id"), Some(&json!(4)));
    }
}
