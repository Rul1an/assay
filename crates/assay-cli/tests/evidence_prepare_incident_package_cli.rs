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
fn record_schema_names_are_not_admitted_sidecar_format_tokens() {
    let session = wrap_session(Wire::Legacy, "assay://k1-receipts");
    let receipts = session.path("receipts");
    assert!(
        fs::metadata(&receipts).unwrap().len() > 0,
        "wrap must write the receipts sidecar"
    );
    for format in [
        "request-receipt-v0",
        "assay.request_decision_receipt.v0",
        "assay.request_observation.v0",
    ] {
        refuse_reason(
            &session.path("decisions"),
            &[(format, receipts.as_path())],
            "unknown_format",
        );
    }
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

#[test]
fn request_sidecar_opt_in_retains_original_bytes_without_new_units() {
    let session = wrap_session(Wire::Legacy, "assay://sidecar-context");
    let out = session.path("with-sidecar.tar");
    let original = fs::read(session.path("receipts")).unwrap();
    let prepared = report(
        prepare()
            .arg("--decision-log")
            .arg(session.path("decisions"))
            .arg("--request-receipts")
            .arg(session.path("receipts"))
            .arg("--out")
            .arg(&out),
        0,
    );
    assert_eq!(prepared["counts"]["units"], 4);
    let verified = report(
        Command::cargo_bin("assay")
            .unwrap()
            .args(["evidence", "verify-incident-package"])
            .arg(&out),
        0,
    );
    assert_eq!(verified["counts"]["units"], 4);
    let mut archive = tar::Archive::new(std::io::Cursor::new(fs::read(out).unwrap()));
    let mut found = false;
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        found |= bytes == original;
    }
    assert!(found, "original sidecar bytes missing from package");
}

#[test]
fn both_sidecar_options_bound_the_file_before_reading_at_selected_transcript_limit() {
    let session = wrap_session(Wire::Legacy, "assay://sidecar-budget");
    let huge = session.path("sparse-sidecar");
    fs::File::create(&huge)
        .unwrap()
        .set_len(assay_evidence::ContextLimits::HARD.object_bytes_total + 1)
        .unwrap();
    let mut context = assay_evidence::ContextInput::default();
    context.limits.transcript_bytes = 1024 * 1024;
    let context_path = session.path("context.json");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    for generic in [false, true] {
        let out = session.path(if generic {
            "generic.tar"
        } else {
            "dedicated.tar"
        });
        let mut cmd = prepare();
        cmd.arg("--decision-log")
            .arg(session.path("decisions"))
            .arg("--context")
            .arg(&context_path)
            .arg("--out")
            .arg(&out);
        if generic {
            cmd.arg("--input").arg(format!(
                "assay-request-receipts-ndjson-v0={}",
                huge.display()
            ));
        } else {
            cmd.arg("--request-receipts").arg(&huge);
        }
        assert_eq!(
            report(&mut cmd, 2)["reason"],
            "resource_selected",
            "generic={generic}"
        );
        assert!(!out.exists());
    }
}

// #3495: attestation inputs through the published command shape. The bundle is
// the checked-in fixture's bundle object; the key pair, the DSSE and the
// attestation report come from the built `assay` commands, not from test code.

struct Attested {
    dir: TempDir,
}

impl Attested {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

fn assay() -> Command {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.timeout(Duration::from_secs(20));
    cmd
}

fn attested_bundle() -> Attested {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assay-evidence/tests/fixtures/incident_package/valid_five_surfaces.tar");
    let container = assay_evidence::read_incident_container(&fs::read(fixture).unwrap())
        .expect("fixture container");
    let inventory: Value = serde_json::from_slice(container.inventory_bytes()).unwrap();
    let bundle_sha = inventory["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["id"] == "bundle")
        .expect("fixture bundle input")["sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let bundle = container
        .objects()
        .iter()
        .find(|object| object.name == format!("objects/{bundle_sha}"))
        .expect("fixture bundle object")
        .bytes
        .clone();
    fs::write(dir.path().join("bundle.tar.gz"), bundle).unwrap();
    assay()
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(dir.path().join("keys"))
        .assert()
        .success();
    assay()
        .args(["evidence", "attest", "--bundle"])
        .arg(dir.path().join("bundle.tar.gz"))
        .arg("--key")
        .arg(dir.path().join("keys/private_key.pem"))
        .arg("--out")
        .arg(dir.path().join("att.dsse.json"))
        .assert()
        .success();
    let report = assay()
        .args(["evidence", "verify-attestation", "--bundle"])
        .arg(dir.path().join("bundle.tar.gz"))
        .arg("--attestation")
        .arg(dir.path().join("att.dsse.json"))
        .arg("--key")
        .arg(dir.path().join("keys/public_key.pem"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(dir.path().join("report.json"), report).unwrap();
    let key_sha = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(
            fs::read(dir.path().join("keys/public_key.pem")).unwrap(),
        ))
    };
    let context = assay_evidence::ContextInput {
        keys: vec![key_sha],
        ..assay_evidence::ContextInput::default()
    };
    fs::write(
        dir.path().join("ctx.json"),
        serde_json::to_vec(&context).unwrap(),
    )
    .unwrap();
    Attested { dir }
}

fn prepare_with(
    log: &Path,
    attested: &Attested,
    inputs: &[(&str, &str)],
    context: bool,
    out: &Path,
) -> Command {
    let mut cmd = prepare();
    cmd.arg("--decision-log").arg(log).arg("--out").arg(out);
    for (format, file) in inputs {
        cmd.arg("--input")
            .arg(format!("{format}={}", attested.path(file).display()));
    }
    if context {
        cmd.arg("--context").arg(attested.path("ctx.json"));
    }
    cmd
}

const BUNDLE_KEY_DSSE: [(&str, &str); 3] = [
    ("assay-bundle-v1", "bundle.tar.gz"),
    ("ed25519-public-key-pem", "keys/public_key.pem"),
    ("dsse-attestation", "att.dsse.json"),
];

#[test]
fn attestation_and_report_prepare_and_verify_under_a_key_selecting_context() {
    let session = wrap_session(Wire::Legacy, "assay://k1-attested");
    let attested = attested_bundle();
    let mut inputs = BUNDLE_KEY_DSSE.to_vec();
    inputs.push(("attestation-report", "report.json"));
    let out = attested.path("package.tar");
    let prepared = report(
        &mut prepare_with(&session.path("decisions"), &attested, &inputs, true, &out),
        0,
    );
    assert_eq!(prepared["outcome"], "package_verified", "{prepared}");
    assert_eq!(prepared["attestations"][0]["status"], "verified");
    let verified = report(
        assay()
            .args(["evidence", "verify-incident-package"])
            .arg(&out)
            .arg("--context")
            .arg(attested.path("ctx.json")),
        0,
    );
    assert_eq!(verified["outcome"], "package_verified", "{verified}");
    assert_eq!(verified, prepared, "fresh verify equals the prepare report");
}

#[test]
fn attestation_without_a_key_selecting_context_refuses_trust_input() {
    let session = wrap_session(Wire::Legacy, "assay://k1-attested-nokey");
    let attested = attested_bundle();
    let out = attested.path("package.tar");
    let refused = report(
        &mut prepare_with(
            &session.path("decisions"),
            &attested,
            &BUNDLE_KEY_DSSE,
            false,
            &out,
        ),
        2,
    );
    assert_eq!(refused["reason"], "trust_input", "{refused}");
    assert_eq!(refused["attestations"][0]["status"], "refused");
    assert!(!out.exists(), "no archive on refusal");
}

#[test]
fn ambiguous_attestation_selection_refuses_with_a_stated_rule() {
    let session = wrap_session(Wire::Legacy, "assay://k1-attested-ambiguous");
    let attested = attested_bundle();
    let out = attested.path("package.tar");
    let mut cmd = prepare_with(
        &session.path("decisions"),
        &attested,
        &[
            ("assay-bundle-v1", "bundle.tar.gz"),
            ("dsse-attestation", "att.dsse.json"),
        ],
        true,
        &out,
    );
    let output = cmd.assert().code(2).get_output().clone();
    let refused: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refused["reason"], "input_shape", "{refused}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("binds an attestation only when")
            && stderr.contains("export-incident-package"),
        "stderr names the rule and the export route: {stderr}"
    );
    assert!(!out.exists(), "no archive on refusal");
}

#[test]
fn input_shape_without_an_attestation_input_prints_no_binding_hint() {
    let session = wrap_session(Wire::Legacy, "assay://k1-no-attestation");
    let log = session.path("decisions");
    let mut bytes = fs::read(&log).unwrap();
    let last = bytes.len() - 2;
    bytes[last] ^= 0x01;
    fs::write(&log, bytes).unwrap();
    let out = session.path("package.tar");
    let output = prepare()
        .arg("--decision-log")
        .arg(&log)
        .arg("--out")
        .arg(&out)
        .assert()
        .code(2)
        .get_output()
        .clone();
    let refused: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refused["reason"], "input_shape", "{refused}");
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains("binds an attestation"),
        "the hint is only for selections with an attestation input"
    );
}
