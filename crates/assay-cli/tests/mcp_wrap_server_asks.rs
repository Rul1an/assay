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
const SERVER: &str = r#"
import sys, json
MODE = sys.argv[1]
def out(o):
    print(json.dumps(o, separators=(',', ':')), flush=True)
pending = None
calls = []
for line in sys.stdin:
    r = json.loads(line)
    if 'method' not in r:
        if MODE == 'legacy_ask' and r.get('id') == 'ask-1':
            out({'jsonrpc':'2.0','id':pending,'result':{'content':[{'type':'text','text':r['result']['action']}]}})
        continue
    k = r['method']
    i = r.get('id')
    if k == 'initialize':
        ver = '2026-07-28' if MODE.startswith('modern') else '2025-11-25'
        out({'jsonrpc':'2.0','id':i,'result':{'protocolVersion':ver,'capabilities':{'tools':{}},'serverInfo':{'name':'s','version':'1'}}})
    elif k == 'tools/call':
        if MODE == 'legacy_ask':
            pending = i
            out({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'Delete repository prod?','requestedSchema':{'type':'object','properties':{}}}})
        elif MODE == 'modern_ask':
            if r.get('params', {}).get('inputResponses'):
                out({'jsonrpc':'2.0','id':i,'result':{'resultType':'complete','content':[{'type':'text','text':'done'}]}})
            else:
                out({'jsonrpc':'2.0','id':i,'result':{'resultType':'input_required','inputRequests':{'c':{'method':'elicitation/create','params':{'message':'Sure?','requestedSchema':{'type':'object','properties':{}}}}},'requestState':'st1'}})
        elif MODE == 'malformed_legacy':
            print('{"jsonrpc":"2.0","id":"ask-1","method":"elicitation/create","params":[]}', flush=True)
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
        elif MODE == 'malformed_modern':
            out({'jsonrpc':'2.0','id':i,'result':{'resultType':'input_required','inputRequests':['not-an-object'],'requestState':'st1'}})
        elif MODE == 'two_pending':
            calls.append(i)
            if len(calls) == 2:
                out({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'Two pending?','requestedSchema':{'type':'object','properties':{}}}})
        elif MODE == 'sampling_after':
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
            out({'jsonrpc':'2.0','id':'samp-1','method':'sampling/createMessage','params':{'messages':[{'role':'user','content':{'type':'text','text':'unlinked sampling'}}]}})
        elif MODE == 'oversize_ask':
            print(json.dumps({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'x'*1048576}}, separators=(',', ':')), flush=True)
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
        else:
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
    elif i is not None:
        out({'jsonrpc':'2.0','id':i,'result':{}})
"#;

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
    let dir = TempDir::new().unwrap();
    let root: &Path = dir.path();
    fs::write(root.join("policy.yaml"), POLICY).unwrap();
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
