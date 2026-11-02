//! Wrap-session outputs become an incident package the existing verifier accepts.
//!
//! Decision logs are produced by the built `assay mcp wrap` against a fixture
//! server. No decision line in these tests is hand-written.

#![cfg(unix)]

use assert_cmd::Command;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};
use tempfile::TempDir;

const SERVER: &str = r#"
import sys, json
for line in sys.stdin:
    r = json.loads(line)
    i = r.get("id")
    if r.get("method") == "tools/list":
        print(json.dumps({"jsonrpc": "2.0", "id": i, "result": {"tools": [
            {"name": "deploy", "description": "Deploy.", "inputSchema": {"type": "object"}}
        ]}}), flush=True)
    elif i is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": i, "result": {"content": []}}), flush=True)
"#;

#[derive(Clone, Copy)]
enum Wire {
    Legacy,
    Modern,
}

impl Wire {
    fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Modern => "2026-07-28",
        }
    }

    fn initialize(self) -> Value {
        let version = match self {
            Self::Legacy => "2024-11-05",
            Self::Modern => "2026-07-28",
        };
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": {"name": "k1", "version": "1"}
            }
        })
    }

    fn call(self, id: u64, name: &str, arguments: Value) -> Value {
        let mut params = json!({"name": name, "arguments": arguments});
        if matches!(self, Self::Modern) {
            params["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {}
            });
        }
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params})
    }
}

struct WrapOutputs {
    dir: TempDir,
}

impl WrapOutputs {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn decisions(&self) -> Vec<Value> {
        fs::read_to_string(self.path("decisions"))
            .unwrap()
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn write_line(stdin: &mut impl Write, value: &Value) {
    writeln!(stdin, "{value}").unwrap();
    stdin.flush().unwrap();
}

fn wrap_cmd(dir: &Path, event_source: &str) -> StdCommand {
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            event_source,
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
        ]);
    cmd
}

fn pin_deploy(dir: &Path) -> (String, String, String) {
    fs::write(
        dir.join("policy.yaml"),
        "version: '2.0'\nname: k1-pins\ntools:\n  allow: [deploy]\n",
    )
    .unwrap();
    fs::write(dir.join("server.py"), SERVER).unwrap();
    let mut child = wrap_cmd(dir, "assay://k1-pin-probe").spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        write_line(
            stdin,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
        );
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let listed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let identity = &listed["result"]["tools"][0]["tool_identity"];
    (
        identity["server_id"].as_str().unwrap().to_string(),
        identity["schema_hash"].as_str().unwrap().to_string(),
        identity["meta_hash"].as_str().unwrap().to_string(),
    )
}

fn wrap_session(wire: Wire, event_source: &str) -> WrapOutputs {
    let pin_dir = TempDir::new().unwrap();
    let (server_id, schema_hash, meta_hash) = pin_deploy(pin_dir.path());
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(
        root.join("policy.yaml"),
        format!(
            "version: '2.0'\nname: k1-wrap-incident\ntools:\n  allow: [read, redact_me, deploy]\n  deny: [blocked]\n  redact_args: [redact_me]\n  redact_args_contract:\n    redaction_target: body\n    redaction_mode: mask\n    redaction_scope: args\ntool_pins:\n  deploy:\n    server_id: {server_id}\n    tool_name: deploy\n    schema_hash: {schema_hash}\n    meta_hash: {meta_hash}\n"
        ),
    )
    .unwrap();
    fs::write(root.join("server.py"), SERVER).unwrap();
    let mut child = wrap_cmd(root, event_source).spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        write_line(stdin, &wire.initialize());
        write_line(
            stdin,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        );
        write_line(stdin, &wire.call(2, "deploy", json!({})));
        write_line(stdin, &wire.call(3, "blocked", json!({})));
        write_line(stdin, &wire.call(4, "read", json!({})));
        write_line(
            stdin,
            &wire.call(5, "redact_me", json!({"body": "secret-token"})),
        );
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "wrap {}: {output:?}", wire.name());
    let session = WrapOutputs { dir };
    let decisions = session.decisions();
    assert_eq!(
        decisions.len(),
        4,
        "{}",
        session.path("decisions").display()
    );
    let data: Vec<&Value> = decisions.iter().map(|event| &event["data"]).collect();
    assert_eq!(data[0]["tool"], "deploy");
    assert_eq!(data[0]["decision"], "deny");
    assert_eq!(data[0]["reason_code"], "P_TOOL_UNOBSERVED");
    assert_eq!(data[1]["tool"], "blocked");
    assert_eq!(data[1]["decision"], "deny");
    assert_eq!(data[2]["tool"], "read");
    assert_eq!(data[2]["decision"], "allow");
    assert_eq!(data[3]["tool"], "redact_me");
    assert_eq!(data[3]["decision"], "allow");
    assert_eq!(data[3]["redaction_applied_state"], "applied");
    session
}

fn prepare() -> Command {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.args(["evidence", "prepare-incident-package"])
        .timeout(Duration::from_secs(20));
    cmd
}

fn report(cmd: &mut Command, code: i32) -> Value {
    let output = cmd.assert().code(code).get_output().stdout.clone();
    serde_json::from_slice(&output).expect("incident verify report on stdout")
}

fn prepare_and_verify(log: &Path, out: &Path) -> Value {
    let prepared = report(
        prepare()
            .arg("--decision-log")
            .arg(log)
            .arg("--out")
            .arg(out),
        0,
    );
    assert_eq!(prepared["outcome"], "package_verified", "{prepared}");
    let verified = report(
        Command::cargo_bin("assay")
            .unwrap()
            .args(["evidence", "verify-incident-package"])
            .arg(out)
            .timeout(Duration::from_secs(20)),
        0,
    );
    assert_eq!(verified["outcome"], "package_verified", "{verified}");
    verified
}

fn refuse_reason(log: &Path, extra: &[(&str, &Path)], reason: &str) {
    let dir = log.parent().unwrap();
    let mut cmd = prepare();
    cmd.arg("--decision-log")
        .arg(log)
        .arg("--out")
        .arg(dir.join("refused.tar"));
    for (format, path) in extra {
        cmd.arg("--input")
            .arg(format!("{format}={}", path.display()));
    }
    let refused = report(&mut cmd, 2);
    assert_eq!(refused["schema"], "assay.incident.verify.v1");
    assert_eq!(refused["outcome"], "package_refused", "{refused}");
    assert_eq!(refused["reason"], reason, "{refused}");
}

#[test]
fn wrap_legacy_prepare_verifies_on_receiver_default_context() {
    let session = wrap_session(Wire::Legacy, "assay://k1-legacy");
    let out = session.path("package.tar");
    let verified = prepare_and_verify(&session.path("decisions"), &out);
    assert_eq!(verified["counts"]["units"], 4);
    assert_eq!(verified["counts"]["examined"], 4);
}

#[test]
fn wrap_modern_wire_prepare_verifies_on_receiver_default_context() {
    let session = wrap_session(Wire::Modern, "assay://k1-modern");
    let out = session.path("package.tar");
    let verified = prepare_and_verify(&session.path("decisions"), &out);
    assert_eq!(verified["counts"]["units"], 4);
    assert_eq!(verified["counts"]["examined"], 4);
}

#[test]
fn flipped_byte_in_decision_log_refuses_input_shape() {
    let session = wrap_session(Wire::Legacy, "assay://k1-flip");
    let log = session.path("flipped");
    let mut bytes = fs::read(session.path("decisions")).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&log, bytes).unwrap();
    refuse_reason(&log, &[], "input_shape");
}

#[test]
fn truncated_decision_log_refuses_input_shape() {
    let session = wrap_session(Wire::Legacy, "assay://k1-trunc");
    let original = fs::read_to_string(session.path("decisions")).unwrap();
    let first = original.lines().next().unwrap();
    assert!(first.len() > 8, "{first}");
    let log = session.path("truncated");
    fs::write(&log, &first[..first.len() / 2]).unwrap();
    refuse_reason(&log, &[], "input_shape");
}

#[test]
fn extra_unknown_member_in_decision_record_refuses_input_shape() {
    let session = wrap_session(Wire::Legacy, "assay://k1-extra");
    let original = fs::read_to_string(session.path("decisions")).unwrap();
    let mut lines: Vec<Value> = original
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    lines[0]
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), json!(true));
    let log = session.path("extra");
    let mut body = String::new();
    for line in lines {
        body.push_str(&serde_json::to_string(&line).unwrap());
        body.push('\n');
    }
    fs::write(&log, body).unwrap();
    refuse_reason(&log, &[], "input_shape");
}

#[test]
fn decision_line_from_another_session_refuses_input_shape() {
    let session = wrap_session(Wire::Legacy, "assay://k1-session-a");
    let other = wrap_session(Wire::Legacy, "assay://k1-session-b");
    let mut combined = fs::read_to_string(session.path("decisions")).unwrap();
    let foreign = other.decisions().into_iter().next().unwrap();
    combined.push_str(&serde_json::to_string(&foreign).unwrap());
    combined.push('\n');
    let log = session.path("mixed");
    fs::write(&log, combined).unwrap();
    refuse_reason(&log, &[], "input_shape");
}

#[test]
fn request_receipts_sidecar_is_not_an_admitted_input_unknown_format() {
    let session = wrap_session(Wire::Legacy, "assay://k1-receipts");
    let receipts = session.path("receipts");
    assert!(
        fs::metadata(&receipts).unwrap().len() > 0,
        "wrap must write the receipts sidecar"
    );
    refuse_reason(
        &session.path("decisions"),
        &[("request-receipt-v0", receipts.as_path())],
        "unknown_format",
    );
}

#[test]
fn help_states_the_non_claims() {
    let help = String::from_utf8(
        prepare()
            .arg("--help")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    for needle in [
        "not a count of all calls",
        "not completeness of capture",
        "not proof of what the server did",
        "an allow is not proof a human accepted",
    ] {
        assert!(
            help.contains(needle),
            "missing non-claim {needle:?} in {help}"
        );
    }
}
