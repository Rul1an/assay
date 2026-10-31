//! `assay mcp wrap` must keep what `approval_required`, `restrict_scope` and `redact_args` promise:
//! what reaches the server is read at the server, and each record is read beside it.

use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

const SENTINEL: &str = "hunter2-sentinel";
/// Appends each frame's exact bytes to `seen`, then answers.
const SERVER: &str = "import sys,json\nfor line in sys.stdin:\n open('seen','a',newline='').write(line)\n r=json.loads(line)\n if 'id' in r: print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'content':[]}}),flush=True)\n";
const SCOPE: &str = "  restrict_scope: [deploy]\n  restrict_scope_contract:\n    scope_type: resource\n    scope_value: /workspace\n    scope_match_mode: prefix\n";
const APPROVAL: &str = "  approval_required: [deploy]\n";

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

struct Run {
    _dir: TempDir,
    output: Output,
    seen: String,
    decisions: Vec<Value>,
    receipts: String,
    decision_log: String,
}

fn call_line(arguments: &Value) -> String {
    format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"deploy","arguments":arguments}})
    )
}

fn wrap(controls: &str, line: &str, extra: &[&str]) -> Run {
    let dir = TempDir::new().unwrap();
    let root: &Path = dir.path();
    fs::write(
        root.join("policy.yaml"),
        format!("version: '2.0'\nname: obligations\ntools:\n{controls}"),
    )
    .unwrap();
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
            "assay://obligations-test",
            "--decision-log",
            "decisions",
            "--request-receipts-out",
            "receipts",
        ])
        .args(extra)
        .args(["--", python(), "-I", "server.py"])
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let decision_log = fs::read_to_string(root.join("decisions")).unwrap_or_default();
    let decisions = decision_log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["data"].clone())
        .collect();
    Run {
        seen: fs::read_to_string(root.join("seen")).unwrap_or_default(),
        receipts: fs::read_to_string(root.join("receipts")).unwrap_or_default(),
        decisions,
        decision_log,
        output,
        _dir: dir,
    }
}

impl Run {
    fn decision(&self) -> &Value {
        assert_eq!(self.decisions.len(), 1, "{:?}", self.decisions);
        &self.decisions[0]
    }

    fn outcome(&self, kind: &str) -> &Value {
        self.decision()["obligation_outcomes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|outcome| outcome["type"] == kind)
            .unwrap_or_else(|| panic!("no {kind} outcome: {:?}", self.decision()))
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn assert_denied(&self, reason_code: &str) {
        assert!(
            self.output.status.success(),
            "{}",
            String::from_utf8_lossy(&self.output.stderr)
        );
        assert_eq!(self.seen, "", "a denied call reached the server");
        assert_eq!(self.decision()["decision"], "deny");
        assert_eq!(self.decision()["reason_code"], reason_code);
        let reply: Value = serde_json::from_str(self.stdout().trim()).unwrap();
        assert_eq!(reply["id"], 2);
        assert_eq!(reply["result"]["isError"], true);
    }

    fn assert_forwarded_exactly(&self, line: &str) {
        assert_eq!(self.seen, line);
        assert_eq!(self.decision()["decision"], "allow");
    }
}

fn approval(tool: &str, resource: &str, expires_at: &str) -> Value {
    json!({
        "approval_id": "approval-1",
        "approver": "reviewer",
        "issued_at": "2020-01-01T00:00:00Z",
        "expires_at": expires_at,
        "scope": "deploy",
        "bound_tool": tool,
        "bound_resource": resource,
    })
}

const FAR_FUTURE: &str = "2999-01-01T00:00:00Z";

#[test]
fn controls_keep_their_bytes() {
    let line = call_line(&json!({"resource": "/etc/passwd", "body": SENTINEL}));
    wrap("  allow: [deploy]\n", &line, &[]).assert_forwarded_exactly(&line);
    wrap("  deny: [deploy]\n", &line, &[]).assert_denied("P_TOOL_DENIED");
}

#[test]
fn approval_required_without_a_valid_artifact_never_reaches_the_server() {
    let cases = [
        ("missing", json!({"resource": "prod"}), "missing approval"),
        (
            "expired",
            json!({"resource": "prod", "_meta": {"approval": approval("deploy", "prod", "2021-01-01T00:00:00Z")}}),
            "expired approval",
        ),
        (
            "other tool",
            json!({"resource": "prod", "_meta": {"approval": approval("other", "prod", FAR_FUTURE)}}),
            "bound tool mismatch",
        ),
        (
            "other resource",
            json!({"resource": "prod", "_meta": {"approval": approval("deploy", "staging", FAR_FUTURE)}}),
            "bound resource mismatch",
        ),
    ];
    for (name, arguments, reason) in cases {
        let run = wrap(
            &format!("  allow: [deploy]\n{APPROVAL}"),
            &call_line(&arguments),
            &[],
        );
        run.assert_denied("P_APPROVAL_REQUIRED");
        assert_eq!(run.decision()["approval_state"], "denied", "{name}");
        assert_eq!(run.decision()["approval_failure_reason"], reason, "{name}");
        assert_eq!(
            run.outcome("approval_required")["status"],
            "error",
            "{name}"
        );
    }
}

#[test]
fn a_valid_approval_forwards_the_exact_bytes() {
    let line = call_line(
        &json!({"resource": "prod", "_meta": {"approval": approval("deploy", "prod", FAR_FUTURE)}}),
    );
    let run = wrap(&format!("  allow: [deploy]\n{APPROVAL}"), &line, &[]);
    run.assert_forwarded_exactly(&line);
    assert_eq!(run.decision()["approval_state"], "approved");
    assert_eq!(run.outcome("approval_required")["status"], "applied");
}

#[test]
fn restrict_scope_refuses_what_is_not_inside_the_scope() {
    let cases = [
        ("outside", json!({"resource": "/etc/passwd"})),
        ("absent", json!({"path": "/workspace/a"})),
        ("sibling", json!({"resource": "/workspace-other/a"})),
        ("parent", json!({"resource": "/workspace/../etc/passwd"})),
        ("backslash", json!({"resource": "/workspace/..\\etc"})),
        ("encoded", json!({"resource": "/workspace/%2e%2e/etc"})),
        (
            "disagreeing",
            json!({"resource": "/etc/passwd", "_meta": {"resource": "/workspace/a"}}),
        ),
    ];
    for (name, arguments) in cases {
        let run = wrap(
            &format!("  allow: [deploy]\n{SCOPE}"),
            &call_line(&arguments),
            &[],
        );
        assert_eq!(run.seen, "", "{name} reached the server");
        run.assert_denied("P_RESTRICT_SCOPE");
        assert_eq!(run.outcome("restrict_scope")["status"], "error", "{name}");
    }
}

#[test]
fn restrict_scope_forwards_what_is_inside_the_scope() {
    for resource in ["/workspace", "/workspace/a/b", "/workspace/a..b"] {
        let line = call_line(&json!({"resource": resource}));
        let run = wrap(&format!("  allow: [deploy]\n{SCOPE}"), &line, &[]);
        run.assert_forwarded_exactly(&line);
        assert_eq!(run.decision()["scope_evaluation_state"], "matched");
        assert_eq!(run.outcome("restrict_scope")["status"], "applied");
    }
}

fn redaction(target: &str, mode: &str) -> String {
    format!(
        "  allow: [deploy]\n  redact_args: [deploy]\n  redact_args_contract:\n    redaction_target: {target}\n    redaction_mode: {mode}\n    redaction_scope: args\n"
    )
}

fn assert_no_sentinel_retained(run: &Run) {
    let stderr = String::from_utf8_lossy(&run.output.stderr);
    for (name, text) in [
        ("server", run.seen.as_str()),
        ("decision log", run.decision_log.as_str()),
        ("receipts", run.receipts.as_str()),
        ("stdout", run.stdout().as_str()),
        ("stderr", stderr.as_ref()),
    ] {
        assert!(!text.contains(SENTINEL), "sentinel in {name}: {text}");
    }
}

fn forwarded_arguments(run: &Run) -> Value {
    let frame: Value = serde_json::from_str(run.seen.trim()).unwrap();
    assert_eq!(frame["id"], 2);
    assert_eq!(frame["params"]["name"], "deploy");
    frame["params"]["arguments"].clone()
}

#[test]
fn redact_args_changes_what_reaches_the_server() {
    let line = call_line(&json!({"body": SENTINEL, "keep": "visible"}));
    for (mode, expected) in [
        ("mask", json!({"body": "[REDACTED]", "keep": "visible"})),
        ("partial", json!({"body": "hu***", "keep": "visible"})),
        ("drop", json!({"keep": "visible"})),
    ] {
        let run = wrap(&redaction("body", mode), &line, &[]);
        assert_eq!(forwarded_arguments(&run), expected, "{mode}");
        assert_eq!(run.decision()["redaction_applied_state"], "applied");
        assert_eq!(run.outcome("redact_args")["status"], "applied");
        assert_no_sentinel_retained(&run);
        assert!(run.receipts.contains("request_line"), "{mode}: no receipt");
    }
}

#[test]
fn redact_args_hash_is_not_md5() {
    let run = wrap(
        &redaction("body", "hash"),
        &call_line(&json!({"body": SENTINEL})),
        &[],
    );
    let body = forwarded_arguments(&run)["body"].clone();
    let body = body.as_str().unwrap();
    assert!(body.starts_with("sha256:"), "{body}");
    assert_eq!(body.len(), "sha256:".len() + 64, "{body}");
    assert_no_sentinel_retained(&run);
}

#[test]
fn redacting_the_whole_arguments_keeps_an_object() {
    let line = call_line(&json!({"token": SENTINEL, "nested": {"a": SENTINEL}}));
    let run = wrap(&redaction("args", "mask"), &line, &[]);
    assert_eq!(
        forwarded_arguments(&run),
        json!({"token": "[REDACTED]", "nested": "[REDACTED]"})
    );
    assert_no_sentinel_retained(&run);
}

#[test]
fn redaction_that_cannot_apply_refuses_the_call() {
    let run = wrap(
        &redaction("body", "mask"),
        &call_line(&json!({"token": SENTINEL})),
        &[],
    );
    run.assert_denied("P_REDACT_ARGS");
    assert_eq!(run.decision()["redaction_applied_state"], "not_applied");
    assert_eq!(run.outcome("redact_args")["status"], "error");
    assert_no_sentinel_retained(&run);
}

#[test]
fn a_call_denied_before_redaction_does_not_retain_what_redaction_covers() {
    let controls = format!("{}{APPROVAL}", redaction("body", "mask"));
    let run = wrap(&controls, &call_line(&json!({"body": SENTINEL})), &[]);
    run.assert_denied("P_APPROVAL_REQUIRED");
    assert_no_sentinel_retained(&run);
}

#[test]
fn dry_run_forwards_unchanged_and_does_not_claim_a_redaction() {
    let line = call_line(&json!({"body": SENTINEL}));
    let run = wrap(&redaction("body", "mask"), &line, &["--dry-run"]);
    assert_eq!(run.seen, line);
    assert_eq!(run.decision()["decision"], "allow");
    assert_eq!(run.decision()["redaction_applied_state"], "not_applied");
    assert_eq!(run.outcome("redact_args")["status"], "skipped");
    assert_eq!(run.outcome("redact_args")["reason_code"], "dry_run");
    assert_eq!(run.receipts, "", "dry-run receipt would retain the secret");
    assert!(!run.decision_log.contains(SENTINEL));
}

#[test]
fn dry_run_forwards_what_it_would_refuse() {
    let line = call_line(&json!({"resource": "/etc/passwd"}));
    let run = wrap(
        &format!("  allow: [deploy]\n{SCOPE}{APPROVAL}"),
        &line,
        &["--dry-run"],
    );
    assert_eq!(run.seen, line);
    assert_eq!(run.decision()["decision"], "allow");
    assert_eq!(run.decision()["reason_code"], "P_APPROVAL_REQUIRED");
}

#[test]
fn a_denial_on_the_modern_wire_is_typed() {
    let line = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"deploy","arguments":{"resource":"/etc/passwd"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}})
    );
    let run = wrap(&format!("  allow: [deploy]\n{SCOPE}"), &line, &[]);
    run.assert_denied("P_RESTRICT_SCOPE");
    let reply: Value = serde_json::from_str(run.stdout().trim()).unwrap();
    assert_eq!(reply["result"]["resultType"], "complete");
}
