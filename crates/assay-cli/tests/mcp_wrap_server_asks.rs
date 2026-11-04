//! A reader of wrap's retained outputs (decision log, receipts sidecar, audit log)
//! must be able to tell that a server ask happened during a `tools/call`, which
//! call it belonged to by identifiers already on the wire, the ask's original
//! bytes, and the answer's original bytes, and must be able to tell accept from
//! decline. These are observations of bytes that crossed wrap, not a decision
//! and not proof a human answered.

use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};
use tempfile::TempDir;

const POLICY: &str = "version: '2.0'\nname: server-asks\ntools:\n  allow: [del]\n";
const RECEIPT_SCHEMA: &str = "assay.request_decision_receipt.v0";

/// MODE chooses the server's ask behaviour. Compact JSON so the bytes the client
/// sees are the bytes a retained-output reader must find again.
const SERVER: &str = include_str!("fixtures/request_receipts/server_asks.py");

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

struct Session {
    dir: TempDir,
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stderr: Receiver<String>,
}

fn start(mode: &str) -> Session {
    start_with_policy(mode, POLICY)
}

fn start_with_policy(mode: &str, policy: &str) -> Session {
    let dir = TempDir::new().unwrap();
    let root: &Path = dir.path();
    fs::write(root.join("policy.yaml"), policy).unwrap();
    fs::write(root.join("server.py"), SERVER).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://server-asks-test",
            "--decision-log-new",
            "--decision-log",
            "decisions",
            "--request-receipts-out",
            "receipts",
            "--audit-log",
            "audit",
            "--",
            python(),
            "-I",
            "server.py",
            mode,
            "--record-seen",
        ])
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (sender, lines) = channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let (err_sender, stderr_rx) = channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf);
        let _ = err_sender.send(buf);
    });
    Session {
        dir,
        child,
        stdin,
        lines,
        stderr: stderr_rx,
    }
}

struct Finished {
    seen: String,
    original_decision_log: Vec<u8>,
    original_sidecar: Vec<u8>,
    records: Vec<Value>,
    receipts: Vec<Value>,
    decisions: Vec<Value>,
    stderr: String,
}

impl Session {
    fn send(&mut self, frame: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(format!("{frame}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn send_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn receive_line(&mut self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(20))
            .expect("the proxy answered")
    }

    fn handshake(&mut self, protocol_version: &str) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": protocol_version,
                "capabilities": {"elicitation": {}},
                "clientInfo": {"name": "c", "version": "1"}
            }
        }));
        let _ = self.receive_line();
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    fn finish(mut self) -> Finished {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{status:?}");
        let stderr = self
            .stderr
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_default();
        let root = self.dir.path();
        let decisions = ndjson(&root.join("decisions"));
        let receipts = ndjson(&root.join("receipts"));
        let audit = ndjson(&root.join("audit"));
        let mut records = Vec::new();
        records.extend(decisions.iter().cloned());
        records.extend(receipts.iter().cloned());
        records.extend(audit);
        Finished {
            seen: fs::read_to_string(root.join("seen")).unwrap(),
            original_decision_log: fs::read(root.join("decisions")).unwrap_or_default(),
            original_sidecar: fs::read(root.join("receipts")).unwrap(),
            records,
            receipts,
            decisions,
            stderr,
        }
    }
}

fn ndjson(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or(Value::String(line.to_string())))
        .collect()
}

fn string_fields<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|item| string_fields(item, out)),
        Value::Object(map) => map.values().for_each(|item| string_fields(item, out)),
        _ => {}
    }
}

fn holds_original(record: &Value, original: &str) -> bool {
    let want = original.trim_end_matches(['\n', '\r']);
    let mut fields = Vec::new();
    string_fields(record, &mut fields);
    fields
        .iter()
        .any(|field| field.trim_end_matches(['\n', '\r']) == want)
}

fn contains_value(value: &Value, needle: &Value) -> bool {
    if value == needle {
        return true;
    }
    match value {
        Value::Array(items) => items.iter().any(|item| contains_value(item, needle)),
        Value::Object(map) => map.values().any(|item| contains_value(item, needle)),
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .is_some_and(|parsed| contains_value(&parsed, needle)),
        _ => false,
    }
}

fn record_holding<'a>(records: &'a [Value], original: &str) -> Option<&'a Value> {
    records
        .iter()
        .find(|record| holds_original(record, original))
}

/// The same retained object carries the original bytes and the wire identifier
/// of the call they belonged to. Inferred pairing across separate objects is
/// not a recorded fact.
fn assert_linked_original(records: &[Value], original: &str, call_id: &Value, what: &str) {
    let record = record_holding(records, original).unwrap_or_else(|| {
        panic!("retained outputs do not hold the original {what} bytes: {original}")
    });
    assert!(
        contains_value(record, call_id),
        "original {what} is retained but not with call id {call_id}: {record}"
    );
}

fn tool_call(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}}
    })
}

fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {"elicitation": {}}
    })
}

fn legacy_answer(action: &str) -> String {
    format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":"ask-1","result":{"action":action,"content":{}}})
    )
}

fn modern_retry(id: u64, action: &str) -> String {
    format!(
        "{}\n",
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "del",
                "arguments": {},
                "_meta": modern_meta(),
                "requestState": "st1",
                "inputResponses": {"c": {"action": action, "content": {}}}
            }
        })
    )
}

fn run_legacy(action: &str) -> (String, String, Finished) {
    let mut session = start("legacy_ask");
    session.handshake("2025-11-25");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}}
    }));
    let ask = session.receive_line();
    let answer = legacy_answer(action);
    session.send_line(&answer);
    let _ = session.receive_line();
    (ask, answer, session.finish())
}

fn run_modern(action: &str) -> (String, String, Finished) {
    let mut session = start("modern_ask");
    session.handshake("2026-07-28");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}, "_meta": modern_meta()}
    }));
    let ask = session.receive_line();
    let answer = modern_retry(5, action);
    session.send_line(&answer);
    let _ = session.receive_line();
    (ask, answer, session.finish())
}

#[test]
fn legacy_accept_and_decline_are_distinguishable_in_retained_outputs() {
    let (ask_accept, answer_accept, accept) = run_legacy("accept");
    let (ask_decline, answer_decline, decline) = run_legacy("decline");
    assert_eq!(ask_accept, ask_decline);
    assert_ne!(answer_accept, answer_decline);
    let call_id = json!(4);
    assert_linked_original(&accept.records, &ask_accept, &call_id, "ask");
    assert_linked_original(&accept.records, &answer_accept, &call_id, "accept answer");
    assert_linked_original(&decline.records, &ask_decline, &call_id, "ask");
    assert_linked_original(
        &decline.records,
        &answer_decline,
        &call_id,
        "decline answer",
    );
    assert!(record_holding(&accept.records, &answer_decline).is_none());
    assert!(record_holding(&decline.records, &answer_accept).is_none());
}

#[test]
fn modern_accept_and_decline_are_distinguishable_in_retained_outputs() {
    let (ask_accept, answer_accept, accept) = run_modern("accept");
    let (ask_decline, answer_decline, decline) = run_modern("decline");
    assert_eq!(ask_accept, ask_decline);
    assert_ne!(answer_accept, answer_decline);
    assert!(
        ask_accept.contains("input_required"),
        "modern ask is the input_required result, not a later retry: {ask_accept}"
    );
    let call_id = json!(4);
    assert_linked_original(&accept.records, &ask_accept, &call_id, "ask");
    assert_linked_original(&accept.records, &answer_accept, &json!(5), "accept answer");
    assert_linked_original(&decline.records, &ask_decline, &call_id, "ask");
    assert_linked_original(
        &decline.records,
        &answer_decline,
        &json!(5),
        "decline answer",
    );
    assert!(record_holding(&accept.records, &answer_decline).is_none());
    assert!(record_holding(&decline.records, &answer_accept).is_none());
}

#[test]
fn no_ask_records_nothing_new() {
    let mut session = start("no_ask");
    session.handshake("2025-11-25");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}}
    }));
    let result = session.receive_line();
    assert!(
        !result.contains("elicitation/create") && !result.contains("input_required"),
        "{result}"
    );
    let finished = session.finish();
    let mut fields = Vec::new();
    for record in &finished.records {
        string_fields(record, &mut fields);
    }
    for field in fields {
        assert!(
            !field.contains("elicitation/create") && !field.contains("input_required"),
            "no-ask run recorded an ask-shaped original: {field}"
        );
    }
}

#[test]
fn malformed_legacy_ask_is_retained_as_failure() {
    let mut session = start("malformed_legacy");
    session.handshake("2025-11-25");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}}
    }));
    let malformed = session.receive_line();
    let _ = session.receive_line();
    let finished = session.finish();
    assert!(malformed.contains("elicitation/create"), "{malformed}");
    let record = record_holding(&finished.records, &malformed)
        .unwrap_or_else(|| panic!("malformed ask bytes were dropped: {malformed}"));
    assert!(
        contains_value(record, &json!(4)),
        "malformed ask is retained but not with the pending call: {record}"
    );
}

#[test]
fn malformed_modern_ask_is_retained_as_failure() {
    let mut session = start("malformed_modern");
    session.handshake("2026-07-28");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}, "_meta": modern_meta()}
    }));
    let malformed = session.receive_line();
    let finished = session.finish();
    assert!(
        malformed.contains("input_required") && malformed.contains("not-an-object"),
        "{malformed}"
    );
    let record = record_holding(&finished.records, &malformed)
        .unwrap_or_else(|| panic!("malformed ask bytes were dropped: {malformed}"));
    assert!(
        contains_value(record, &json!(4)),
        "malformed ask is retained but not with the call id already on the result: {record}"
    );
}

#[test]
fn unmatched_answer_is_retained_without_linking_a_completed_call() {
    let mut session = start("no_ask");
    session.handshake("2025-11-25");
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "del", "arguments": {}}
    }));
    let _ = session.receive_line();
    let unmatched = legacy_answer("accept");
    session.send_line(&unmatched);
    let finished = session.finish();
    let record = record_holding(&finished.records, &unmatched)
        .unwrap_or_else(|| panic!("unmatched answer bytes were dropped: {unmatched}"));
    assert!(
        !contains_value(record, &json!(4)),
        "an answer with no matching ask was recorded as belonging to the earlier call: {record}"
    );
}

/// Two in-flight `tools/call` ids is not a unique observed link. Recording one of
/// them as the owner would be a guess. Unlinked would also pass an XOR of the
/// two ids, so the mark itself is the contract.
#[test]
fn two_pending_calls_leave_a_legacy_ask_ambiguous() {
    let mut session = start("two_pending");
    session.handshake("2025-11-25");
    session.send(&tool_call(4));
    session.send(&tool_call(5));
    let ask = session.receive_line();
    assert!(
        ask.contains("elicitation/create") && ask.contains("Two pending?"),
        "{ask}"
    );
    let finished = session.finish();
    let record = record_holding(&finished.records, &ask)
        .unwrap_or_else(|| panic!("ambiguous ask bytes were dropped: {ask}"));
    let has4 = contains_value(record, &json!(4));
    let has5 = contains_value(record, &json!(5));
    assert_eq!(
        has4, has5,
        "ask was attached to one pending call by guess: {record}"
    );
    assert_eq!(
        record.get("call_link").and_then(Value::as_str),
        Some("ambiguous"),
        "two pending calls must mark the ask ambiguous: {record}"
    );
}

/// Sampling after a completed call has an empty pending set. Sequence is not a link.
#[test]
fn sampling_with_no_pending_call_is_retained_unlinked() {
    let mut session = start("sampling_after");
    session.handshake("2025-11-25");
    session.send(&tool_call(4));
    let _result = session.receive_line();
    let sampling = session.receive_line();
    assert!(
        sampling.contains("sampling/createMessage") && sampling.contains("unlinked sampling"),
        "{sampling}"
    );
    let finished = session.finish();
    let record = record_holding(&finished.records, &sampling)
        .unwrap_or_else(|| panic!("unlinked sampling bytes were dropped: {sampling}"));
    assert!(
        !contains_value(record, &json!(4)),
        "sampling with no pending call was attached to the previous completed call: {record}"
    );
}

/// Ask/answer lines share the sidecar, not the tools/call receipt schema.
#[test]
fn tools_call_receipts_stay_one_per_decision_when_asks_are_present() {
    let (ask, _, finished) = run_legacy("accept");
    let tool_receipts: Vec<&Value> = finished
        .receipts
        .iter()
        .filter(|record| record.get("schema").and_then(Value::as_str) == Some(RECEIPT_SCHEMA))
        .collect();
    assert_eq!(
        tool_receipts.len(),
        finished.decisions.len(),
        "tools/call receipts must stay one per decision; sidecar={:?} decisions={:?}",
        finished.receipts,
        finished.decisions
    );
    let ask_record = record_holding(&finished.receipts, &ask)
        .unwrap_or_else(|| panic!("ask bytes were not in the receipts sidecar: {ask}"));
    assert_ne!(
        ask_record.get("schema").and_then(Value::as_str),
        Some(RECEIPT_SCHEMA),
        "ask must not reuse the tools/call receipt schema: {ask_record}"
    );
}

/// An oversize ask stops capture and says so. Dropping it while the sidecar
/// still looks complete would be a silent omission.
#[test]
fn oversize_ask_is_a_capture_incomplete_failure() {
    let mut session = start("oversize_ask");
    session.handshake("2025-11-25");
    session.send(&tool_call(4));
    let oversize = session.receive_line();
    let _ = session.receive_line();
    assert!(oversize.contains("elicitation/create"), "{oversize}");
    assert!(oversize.len() > 1024 * 1024, "{}", oversize.len());
    let finished = session.finish();
    assert!(
        finished.stderr.contains("capture incomplete"),
        "oversize ask was dropped without a capture-incomplete failure: {}",
        finished.stderr
    );
    assert!(
        record_holding(&finished.receipts, &oversize).is_none(),
        "oversize ask was written through the 1 MiB ceiling"
    );
    assert_eq!(
        finished
            .receipts
            .iter()
            .filter(|record| record.get("schema").and_then(Value::as_str) == Some(RECEIPT_SCHEMA))
            .count(),
        1,
        "the tools/call receipt prefix must be kept: {:?}",
        finished.receipts
    );
}

#[test]
fn mixed_sidecars_survive_incident_handoff_and_tampering_refuses() {
    use assay_evidence::{
        prepare_verified_incident_package, read_incident_container, verify_incident_package,
        ContextInput, IncidentOutcome, IncidentPrepareSource, IncidentReason,
    };
    for modern in [false, true] {
        for action in ["accept", "decline"] {
            let (_, answer, finished) = if modern {
                run_modern_stateless(action)
            } else {
                run_legacy(action)
            };
            let sources = [
                IncidentPrepareSource {
                    id: "decisions".into(),
                    format: "proxy-decision-ndjson".into(),
                    bytes: finished.original_decision_log,
                },
                IncidentPrepareSource {
                    id: "sidecar".into(),
                    format: "assay-request-receipts-ndjson-v0".into(),
                    bytes: finished.original_sidecar,
                },
            ];
            let (mut package, _) =
                prepare_verified_incident_package(&sources, &ContextInput::default()).unwrap();
            let report = verify_incident_package(&package, &ContextInput::default());
            assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
            assert_eq!(
                report.counts.as_ref().unwrap()["units"],
                if modern { 2 } else { 1 }
            );
            let container = read_incident_container(&package).unwrap();
            let assessment: Value = serde_json::from_slice(container.assessment_bytes()).unwrap();
            assert_eq!(assessment["joins"], json!([]));
            assert_eq!(assessment["claims"], json!([]));
            for source in &sources {
                assert_eq!(
                    container
                        .objects()
                        .iter()
                        .filter(|o| o.bytes == source.bytes)
                        .count(),
                    1
                );
            }
            // Legacy answers are observations; modern retries are tools/call
            // receipts. Corrupt that specific row, never an unrelated occurrence.
            let sidecar = &sources[1].bytes;
            let mut answer_offset = None;
            let mut offset = 0;
            for line in sidecar.split_inclusive(|b| *b == b'\n') {
                let row: Value = serde_json::from_slice(line).unwrap();
                let is_answer = if modern {
                    row["schema"] == RECEIPT_SCHEMA && row["rpc_id"] == 5
                } else {
                    row["schema"] == "assay.request_observation.v0" && row["kind"] == "answer"
                };
                if is_answer {
                    assert_eq!(row["request_line"], answer);
                    assert_eq!(
                        if modern {
                            &row["rpc_id"]
                        } else {
                            &row["call_id"]
                        },
                        &json!(if modern { 5 } else { 4 })
                    );
                    let at = line
                        .windows(action.len())
                        .position(|b| b == action.as_bytes())
                        .unwrap();
                    assert!(answer_offset.replace(offset + at).is_none());
                }
                offset += line.len();
            }
            let sidecar_offset = package
                .windows(sidecar.len())
                .position(|b| b == sidecar)
                .unwrap();
            let before = package.clone();
            package[sidecar_offset + answer_offset.unwrap()] ^= 1;
            assert_eq!(
                before.iter().zip(&package).filter(|(a, b)| a != b).count(),
                1
            );
            assert_eq!(
                verify_incident_package(&package, &ContextInput::default()).reason,
                Some(IncidentReason::InputShape)
            );
        }
    }
}

fn modern_call(id: u64) -> Value {
    let mut call = tool_call(id);
    call["params"]["_meta"] = modern_meta();
    call
}

fn run_modern_stateless(action: &str) -> (String, String, Finished) {
    let mut session = start("modern_ask");
    session.send(&modern_call(4));
    let ask = session.receive_line();
    let answer = modern_retry(5, action);
    session.send_line(&answer);
    let response: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(response["id"], 5);
    assert_eq!(response["result"]["resultType"], "complete");
    (ask, answer, session.finish())
}

fn assert_decision_receipts(finished: &Finished, expected: &[(i64, &str)]) {
    assert_eq!(finished.decisions.len(), expected.len());
    let receipts: Vec<_> = finished
        .receipts
        .iter()
        .filter(|r| r["schema"] == RECEIPT_SCHEMA)
        .collect();
    assert_eq!(receipts.len(), expected.len());
    for ((decision, receipt), (id, verdict)) in
        finished.decisions.iter().zip(receipts).zip(expected)
    {
        assert_eq!(decision["data"]["request_id"], *id);
        assert_eq!(decision["data"]["tool_call_id"], format!("req_{id}"));
        assert_eq!(decision["data"]["decision"], *verdict);
        assert_eq!(receipt["rpc_id"], *id);
        assert_eq!(receipt["decision_event_id"], decision["id"]);
    }
}

#[test]
fn stateless_modern_retry_has_separate_decisions_and_original_observations() {
    for action in ["accept", "decline"] {
        let (ask, answer, finished) = run_modern_stateless(action);
        assert_eq!(finished.seen, format!("{}\n{answer}", modern_call(4)));
        assert_decision_receipts(&finished, &[(4, "allow"), (5, "allow")]);
        assert_eq!(finished.receipts.len(), 3);
        for (sequence, row) in finished.receipts.iter().enumerate() {
            assert_eq!(row["sequence"], sequence);
            if row["schema"] == RECEIPT_SCHEMA {
                assert_eq!(row["declared_protocol_version_state"], "present");
                assert_eq!(row["declared_protocol_version"], "2026-07-28");
                let expected = if row["rpc_id"] == 4 {
                    format!("{}\n", modern_call(4))
                } else {
                    answer.clone()
                };
                assert_eq!(row["request_line"], expected);
            }
        }
        for (kind, id, original) in [("ask", 4, format!("{ask}\n")), ("answer", 5, answer)] {
            let rows: Vec<_> = finished
                .receipts
                .iter()
                .filter(|r| {
                    if kind == "ask" {
                        r["schema"] == "assay.request_observation.v0" && r["kind"] == "ask"
                    } else {
                        r["schema"] == RECEIPT_SCHEMA && r["rpc_id"] == id
                    }
                })
                .collect();
            assert_eq!(rows.len(), 1);
            if kind == "ask" {
                assert_eq!(rows[0]["call_link"], "unique");
                assert_eq!(rows[0]["call_id"], id);
            } else {
                assert_eq!(rows[0]["rpc_id"], id);
            }
            assert_eq!(rows[0]["request_line"], original);
            let raw: Value = serde_json::from_str(&original).unwrap();
            let payload = if kind == "ask" {
                &raw["result"]
            } else {
                &raw["params"]
            };
            assert_eq!(payload["requestState"], "st1");
        }
    }
}

#[test]
fn modern_retry_is_denied_before_forwarding_under_deny_policy() {
    let mut session = start_with_policy(
        "modern_ask",
        "version: '2.0'\nname: retry-deny\ntools:\n  deny: [del]\n",
    );
    session.send_line(&modern_retry(5, "accept"));
    let response: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(response["id"], 5);
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], true);
    let finished = session.finish();
    assert!(finished.seen.is_empty());
    assert_decision_receipts(&finished, &[(5, "deny")]);
}

#[test]
fn modern_retry_consumes_tool_call_limit_and_is_not_forwarded_when_exhausted() {
    let policy = format!("{POLICY}limits:\n  max_tool_calls_total: 1\n");
    let mut session = start_with_policy("modern_ask", &policy);
    session.send(&modern_call(4));
    let ask: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(ask["result"]["resultType"], "input_required");
    session.send_line(&modern_retry(5, "accept"));
    let response: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(response["id"], 5);
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], true);
    let finished = session.finish();
    assert_eq!(finished.seen, format!("{}\n", modern_call(4)));
    assert_decision_receipts(&finished, &[(4, "allow"), (5, "deny")]);
    assert_eq!(finished.decisions[1]["data"]["reason_code"], "P_RATE_LIMIT");
}

#[test]
fn legacy_answer_does_not_consume_tool_call_limit_but_next_call_does() {
    let policy = format!("{POLICY}limits:\n  max_tool_calls_total: 1\n");
    let mut session = start_with_policy("legacy_ask", &policy);
    session.handshake("2025-11-25");
    session.send(&tool_call(4));
    let _ask = session.receive_line();
    let answer = legacy_answer("accept");
    session.send_line(&answer);
    let response: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(response["id"], 4);
    assert_eq!(response["result"]["content"][0]["text"], "accept");
    session.send(&tool_call(5));
    let denied: Value = serde_json::from_str(&session.receive_line()).unwrap();
    assert_eq!(denied["id"], 5);
    assert_eq!(denied["result"]["isError"], true);
    let finished = session.finish();
    let seen: Vec<Value> = finished
        .seen
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        seen.iter().filter(|r| r["method"] == "tools/call").count(),
        1
    );
    assert!(finished.seen.contains(&answer));
    assert_decision_receipts(&finished, &[(4, "allow"), (5, "deny")]);
    assert_eq!(finished.decisions[1]["data"]["reason_code"], "P_RATE_LIMIT");
}

#[test]
fn task_queries_pass_unchanged_without_decisions_or_receipts() {
    let mut session = start("tasks");
    let mut sent = String::new();
    for (id, method) in [(7, "tasks/get"), (8, "tasks/result")] {
        let line = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":{"taskId":"task-1","_meta":modern_meta()}})
        );
        session.send_line(&line);
        sent.push_str(&line);
        let response: Value = serde_json::from_str(&session.receive_line()).unwrap();
        assert_eq!(response["id"], id);
        assert_eq!(response["result"]["taskId"], "task-1");
    }
    let finished = session.finish();
    assert_eq!(finished.seen, sent);
    assert!(finished.decisions.is_empty());
    assert!(finished.receipts.is_empty());
}
