//! SECURITY-001: the value authorized must be the value forwarded.
//!
//! Drives a real `tools/call` through `proxy-enforce` and compares the PDP
//! decision record against first-member / last-member / strict-reject oracles
//! applied to the bytes actually written upstream. Abstract member fixtures
//! only; not a named-upstream bypass recipe.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const PROXY_FAILED: i64 = -31998;

const ALLOW_ACME: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-deploy"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn mock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/mock_upstream.py")
}

fn oracle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/member_oracles.py")
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json")
}

fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write");
    p
}

fn spawn_enforce(log: &Path, raw: &Path, policy: &Path, decisions: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            mock_path().to_str().unwrap(),
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            baseline_path().to_str().unwrap(),
            "--enforcement-decision-out",
            decisions.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_RAW_LOG", raw)
        .env("MOCK_UPSTREAM_MODE", "p60a")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy")
}

fn init() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    })
}

fn handshake(out: &mut Conn) {
    out.send(init());
    let _ = out.read_response();
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let _ = out.read_response();
}

fn oracle(mode: &str, line: &str) -> Value {
    let out = Command::new(python())
        .args([oracle_path().to_str().unwrap(), "interpret", mode])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn oracle");
    let mut child = out;
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().expect("oracle exit");
    assert!(output.status.success(), "oracle {mode} failed");
    serde_json::from_slice(&output.stdout).expect("oracle json")
}

fn forwarded_calls(raw: &Path) -> Vec<String> {
    std::fs::read_to_string(raw)
        .unwrap_or_default()
        .lines()
        .filter(|l| {
            l.contains("\"method\":\"tools/call\"") || l.contains("\"method\": \"tools/call\"")
        })
        .map(str::to_string)
        .collect()
}

fn enforcement_decision_count(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

fn last_decision(path: &Path) -> Option<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("decision json"))
        .next_back()
}

fn assert_oracles_match_authorized(authorized_name: &str, authorized_args: &Value, line: &str) {
    for mode in ["first", "last", "reject"] {
        let seen = oracle(mode, line);
        assert_eq!(
            seen["accepted"], true,
            "{mode} oracle must accept a unique-member frame: {seen}"
        );
        assert_eq!(
            seen["name"].as_str(),
            Some(authorized_name),
            "{mode} oracle name diverged from the authorized name"
        );
        assert_eq!(
            &seen["arguments"], authorized_args,
            "{mode} oracle arguments diverged from the authorized arguments"
        );
    }
}

const INIT_LINE: &str = r#"{"jsonrpc":"2.0","id":21,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
const LIST_LINE: &str = r#"{"jsonrpc":"2.0","id":22,"method":"tools/list"}"#;
const NOTE_LINE: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const NON_REQUEST_LINE: &str = r#"{"jsonrpc":"2.0","id":23,"result":{"passthrough":true}}"#;
const PING_LINE: &str = r#"{"jsonrpc":"2.0","id":24,"method":"ping"}"#;

#[test]
fn protocol_control_is_forwarded_without_enforcement_decision() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));

    out.send_line(INIT_LINE);
    let init = out.read_response_for_id(21);
    assert!(
        init.get("error").is_none(),
        "initialize must be forwarded, not policy-denied: {init}"
    );
    out.send_line(LIST_LINE);
    let listed = out.read_response_for_id(22);
    assert!(
        listed.get("error").is_none(),
        "tools/list must be forwarded, not policy-denied: {listed}"
    );
    out.send_line(NOTE_LINE);
    out.send_line(NON_REQUEST_LINE);
    out.send_line(PING_LINE);
    let ping = out.read_response_for_id(24);
    assert!(
        ping.get("error").is_none(),
        "ping must be forwarded, not policy-denied: {ping}"
    );
    assert_eq!(
        enforcement_decision_count(&decisions),
        0,
        "non-tools/call unique objects must not mint an enforcement decision: {}",
        std::fs::read_to_string(&decisions).unwrap_or_default()
    );

    let call = r#"{"jsonrpc":"2.0","id":25,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}"#;
    out.send_line(call);
    let r = out.read_response_for_id(25);
    assert!(
        r.get("error").is_none(),
        "tools/call must still authorize the exact parsed value: {r}"
    );
    let _ = out.shutdown();

    let body = std::fs::read_to_string(&raw).unwrap_or_default();
    for line in [INIT_LINE, LIST_LINE, NOTE_LINE, NON_REQUEST_LINE, PING_LINE] {
        assert!(
            body.contains(line),
            "protocol-control line must be forwarded byte-identically: missing {line} in {body}"
        );
    }
    assert_eq!(
        forwarded_calls(&raw).len(),
        1,
        "tools/call must still reach upstream once"
    );
    let rec = last_decision(&decisions).expect("tools/call decision");
    assert_eq!(rec["decision"], "allow");
    assert_eq!(rec["tool"]["name"], "github.add_deploy_key");
    assert_eq!(
        enforcement_decision_count(&decisions),
        1,
        "exactly one enforcement decision, for the tools/call"
    );
}

#[test]
fn unique_key_control_authorized_value_equals_forwarded_value() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));
    handshake(&mut out);

    // Numeric domain control: the id must survive as written, not a re-serialized Number.
    let line = r#"{"jsonrpc":"2.0","id":1e2,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}"#;
    out.send_line(line);
    let r = out.read_response();
    assert!(
        r.get("error").is_none(),
        "unique-key control was denied: {r}"
    );
    let _ = out.shutdown();

    let calls = forwarded_calls(&raw);
    assert_eq!(
        calls.len(),
        1,
        "unique-key control must forward once: {calls:?}"
    );
    assert!(
        calls[0].contains("1e2"),
        "forwarded bytes must preserve the written numeric id: {}",
        calls[0]
    );
    let rec = last_decision(&decisions).expect("decision record");
    assert_eq!(rec["decision"], "allow");
    assert_eq!(rec["tool"]["name"], "github.add_deploy_key");
    assert_oracles_match_authorized(
        "github.add_deploy_key",
        &serde_json::json!({"owner": "acme", "repo": "prod-app"}),
        &calls[0],
    );
}

#[test]
fn noop_control_ping_is_not_a_tools_call() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));
    handshake(&mut out);
    out.send(serde_json::json!({"jsonrpc": "2.0", "id": 50, "method": "ping"}));
    let r = out.read_response();
    assert_eq!(r["id"], 50);
    let _ = out.shutdown();
    assert!(forwarded_calls(&raw).is_empty());
    assert!(last_decision(&decisions).is_none());
    let methods = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(methods.contains("ping"));
}

#[test]
fn duplicate_params_name_is_not_forwarded_and_oracles_do_not_diverge() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));
    handshake(&mut out);

    let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}"#;
    out.send_line(line);
    let r = out.read_response();
    let _ = out.shutdown();

    let calls = forwarded_calls(&raw);
    assert!(
        calls.is_empty(),
        "duplicate params.name must not be forwarded: {calls:?}"
    );
    assert_eq!(
        r["error"]["code"], PROXY_FAILED,
        "duplicate members are an ingress refusal, not a silent last-member allow: {r}"
    );
    if let Some(rec) = last_decision(&decisions) {
        assert_ne!(
            rec["decision"], "allow",
            "must not authorize a collapsed tree: {rec}"
        );
    }
}

/// Abstract duplicate-member fixture whose keys are JSON-escaped. It contains
/// no literal `"method"` / `"params"` / `"tool"` token, so a raw method-bearing
/// heuristic does not see it.
const ESCAPED_DUPLICATE_OBJECT: &str = r#"{"jsonrpc":"2.0","id":1,"\u006dethod":"tools/list","\u006dethod":"tools/call","\u0070arams":{"name":"danger","arguments":{}}}"#;

#[test]
fn escaped_duplicate_object_is_not_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));
    handshake(&mut out);

    out.send_line(ESCAPED_DUPLICATE_OBJECT);
    let r = out.read_response();
    let _ = out.shutdown();

    let body = std::fs::read_to_string(&raw).unwrap_or_default();
    assert!(
        !body.contains(ESCAPED_DUPLICATE_OBJECT),
        "escaped duplicate object must not reach upstream: {body}"
    );
    assert!(
        !body.contains(r#"\u006dethod"#),
        "escaped duplicate object must not reach upstream: {body}"
    );
    assert!(
        !body.contains("danger"),
        "escaped duplicate object must not reach upstream: {body}"
    );
    assert_eq!(
        r["error"]["code"], PROXY_FAILED,
        "object-shaped unique-parse fail is an ingress refusal: {r}"
    );
    if let Some(rec) = last_decision(&decisions) {
        assert_ne!(
            rec["decision"], "allow",
            "must not authorize a collapsed tree: {rec}"
        );
    }
}

#[test]
fn duplicate_argument_member_is_not_forwarded_and_oracles_do_not_diverge() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let raw = dir.path().join("raw.log");
    let decisions = dir.path().join("decisions.ndjson");
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let mut out = Conn::attach(spawn_enforce(&log, &raw, &policy, &decisions));
    handshake(&mut out);

    let line = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"other","owner":"acme","repo":"prod-app"}}}"#;
    out.send_line(line);
    let r = out.read_response();
    let _ = out.shutdown();

    let calls = forwarded_calls(&raw);
    assert!(
        calls.is_empty(),
        "duplicate argument members must not be forwarded: {calls:?}"
    );
    assert_eq!(
        r["error"]["code"], PROXY_FAILED,
        "duplicate arguments refused: {r}"
    );
    if let Some(rec) = last_decision(&decisions) {
        assert_ne!(
            rec["decision"], "allow",
            "must not authorize a collapsed tree: {rec}"
        );
    }
}
