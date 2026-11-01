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
    audit: String,
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
            "--audit-log",
            "audit",
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
        audit: fs::read_to_string(root.join("audit")).unwrap_or_default(),
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
        assert_eq!(self.decision()["typed_decision"], "deny");
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
        ("audit log", run.audit.as_str()),
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

const TAXONOMY: &str = "tool_classes:\n  deploy: [risky]\n";

/// Every `tools:` key, observed at wrap level: a new key fails here until it has a row, so a key
/// cannot parse and be recorded without anything having looked at what it does.
#[test]
fn every_tools_key_is_observed_at_wrap_level() {
    let outside = json!({"resource": "/etc/passwd", "body": SENTINEL});
    let scope_contract = "  restrict_scope_contract:\n    scope_type: resource\n    scope_value: /workspace\n    scope_match_mode: prefix\n";
    let redact_contract = "  redact_args_contract:\n    redaction_target: body\n    redaction_mode: mask\n    redaction_scope: args\n";
    let denied: Vec<(&str, String, &str)> = vec![
        ("allow", "  allow: [other]\n".into(), "P_TOOL_NOT_ALLOWED"),
        ("deny", "  deny: [deploy]\n".into(), "P_TOOL_DENIED"),
        (
            "allow_classes",
            format!("  allow_classes: [other]\n{TAXONOMY}"),
            "P_TOOL_NOT_ALLOWED",
        ),
        (
            "deny_classes",
            format!("  allow: [deploy]\n  deny_classes: [risky]\n{TAXONOMY}"),
            "P_TOOL_DENIED",
        ),
        (
            "approval_required",
            format!("  allow: [deploy]\n{APPROVAL}"),
            "P_APPROVAL_REQUIRED",
        ),
        (
            "approval_required_classes",
            format!("  allow: [deploy]\n  approval_required_classes: [risky]\n{TAXONOMY}"),
            "P_APPROVAL_REQUIRED",
        ),
        (
            "restrict_scope",
            format!("  allow: [deploy]\n{SCOPE}"),
            "P_RESTRICT_SCOPE",
        ),
        (
            "restrict_scope_classes",
            format!(
                "  allow: [deploy]\n  restrict_scope_classes: [risky]\n{scope_contract}{TAXONOMY}"
            ),
            "P_RESTRICT_SCOPE",
        ),
    ];
    let redacted: Vec<(&str, String)> = vec![
        ("redact_args", redaction("body", "mask")),
        (
            "redact_args_classes",
            format!(
                "  allow: [deploy]\n  redact_args_classes: [risky]\n{redact_contract}{TAXONOMY}"
            ),
        ),
    ];
    let contracts = ["restrict_scope_contract", "redact_args_contract"];

    let keys: std::collections::BTreeSet<String> =
        serde_json::to_value(assay_core::mcp::policy::ToolPolicy::default())
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
    let covered = denied
        .iter()
        .map(|row| row.0)
        .chain(redacted.iter().map(|row| row.0))
        .chain(contracts)
        .map(ToString::to_string)
        .collect();
    assert_eq!(keys, covered, "a tools key has no wrap-level row");

    for (key, controls, reason_code) in denied {
        let run = wrap(&controls, &call_line(&outside), &[]);
        assert_eq!(run.seen, "", "{key} let the call through");
        run.assert_denied(reason_code);
    }
    for (key, controls) in redacted {
        let run = wrap(&controls, &call_line(&outside), &[]);
        assert_eq!(forwarded_arguments(&run)["body"], "[REDACTED]", "{key}");
        assert_no_sentinel_retained(&run);
    }

    // Each contract decides against what its default would do: the default scope is an exact `*`,
    // and the default redaction masks `body`.
    let line = call_line(&json!({"resource": "/workspace/a"}));
    wrap(&format!("  allow: [deploy]\n{SCOPE}"), &line, &[]).assert_forwarded_exactly(&line);
    let run = wrap(
        &redaction("headers", "mask"),
        &call_line(&json!({"headers": SENTINEL, "body": "visible"})),
        &[],
    );
    assert_eq!(
        forwarded_arguments(&run),
        json!({"headers": "[REDACTED]", "body": "visible"})
    );
}

/// A frame the policy cannot read as a named call has no tool to decide on, so it must not pass as
/// the empty name.
#[test]
fn a_call_with_no_readable_tool_name_never_reaches_the_server() {
    let frames = [
        json!({"name": ["deploy"], "arguments": {"body": SENTINEL}}),
        json!({"name": 1, "arguments": {"body": SENTINEL}}),
        json!({"arguments": {"body": SENTINEL}}),
        json!(["deploy", {"body": SENTINEL}]),
        json!(null),
    ];
    let controls = [
        "  allow: ['*']\n",
        "  deny: [deploy]\n",
        "  allow: ['*']\n  redact_args: [deploy]\n  redact_args_contract:\n    redaction_target: body\n    redaction_mode: mask\n    redaction_scope: args\n",
    ];
    for controls in controls {
        for params in &frames {
            let line = format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":params})
            );
            let run = wrap(controls, &line, &[]);
            assert!(run.output.status.success(), "{params}");
            assert_eq!(run.seen, "", "{params} reached the server");
            let reply: Value = serde_json::from_str(run.stdout().trim()).unwrap();
            assert_eq!(reply["id"], 2, "{params}");
            assert_eq!(reply["error"]["code"], -32602, "{params}");
            assert_no_sentinel_retained(&run);
        }
    }
    let no_params = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\"}\n";
    let run = wrap("  allow: ['*']\n", no_params, &[]);
    assert_eq!(run.seen, "");
    let reply: Value = serde_json::from_str(run.stdout().trim()).unwrap();
    assert_eq!(reply["error"]["code"], -32602);
}

#[test]
fn a_policy_denial_does_not_retain_what_redaction_covers() {
    let line = call_line(&json!({"body": SENTINEL}));
    let schema = format!(
        "{}schemas:\n  deploy:\n    type: object\n    properties:\n      body:\n        type: number\n",
        redaction("body", "mask")
    );
    let run = wrap(&schema, &line, &[]);
    run.assert_denied("P_ARG_SCHEMA");
    assert_no_sentinel_retained(&run);
    assert_eq!(run.receipts, "");

    let controls = redaction("body", "mask").replace("allow: [deploy]", "deny: [deploy]");
    let run = wrap(&controls, &line, &[]);
    run.assert_denied("P_TOOL_DENIED");
    assert_no_sentinel_retained(&run);
    assert_eq!(run.receipts, "");
}

#[test]
fn a_tool_outside_redaction_keeps_its_receipt_and_its_denial_detail() {
    let line = call_line(&json!({
        "body": "not covered",
        "_meta": {"tool_call_id": "tc-kept", "delegation": {"delegated_from": "agent-a"}},
    }));
    let schema = "  allow: [deploy]\n  redact_args: [other]\nschemas:\n  deploy:\n    type: object\n    properties:\n      body:\n        type: number\n";
    let run = wrap(schema, &line, &[]);
    run.assert_denied("P_ARG_SCHEMA");
    assert!(run.receipts.contains("request_line"));
    assert!(run.stdout().contains("violations"), "{}", run.stdout());
    assert_eq!(run.decision()["tool_call_id"], "tc-kept");
    assert_eq!(run.decision()["delegated_from"], "agent-a");
}

/// Every member of `arguments._meta` the proxy or the policy engine reads into a record.
fn meta_holding_the_sentinel() -> Value {
    let mut artifact = approval("deploy", "/workspace/ok", FAR_FUTURE);
    artifact["approval_id"] = json!(SENTINEL);
    json!({
        "tool_call_id": SENTINEL,
        "delegation": {"delegated_from": SENTINEL, "delegation_depth": 2},
        "approval": artifact,
        "note": SENTINEL,
    })
}

#[test]
fn a_target_covering_the_metadata_leaves_none_of_it_in_any_record() {
    let line =
        call_line(&json!({"resource": "/workspace/ok", "_meta": meta_holding_the_sentinel()}));
    for target in ["metadata", "args"] {
        let controls = format!("{}{APPROVAL}{SCOPE}", redaction(target, "mask"));
        let run = wrap(&controls, &line, &[]);
        assert_eq!(forwarded_arguments(&run)["_meta"], "[REDACTED]", "{target}");
        assert_no_sentinel_retained(&run);
        let decision = run.decision();
        assert_eq!(decision["tool_call_id"], "req_2", "{target}");
        assert!(decision["delegation_depth"].is_null(), "{target}");
        assert_eq!(decision["redaction_applied_state"], "applied", "{target}");
    }
}

#[test]
fn a_covered_call_refused_by_any_rule_leaves_no_metadata_in_any_record() {
    let line = call_line(&json!({"resource": "/etc/passwd", "_meta": meta_holding_the_sentinel()}));
    let denied_by_name = redaction("body", "mask").replace("allow: [deploy]", "deny: [deploy]");
    let denied_by_schema = format!(
        "{}schemas:\n  deploy:\n    type: object\n    properties:\n      resource:\n        type: number\n",
        redaction("body", "mask")
    );
    let denied_by_scope = format!("{}{SCOPE}", redaction("metadata", "mask"));
    let mut refusals = vec![
        (denied_by_name, "P_TOOL_DENIED"),
        (denied_by_schema, "P_ARG_SCHEMA"),
        (denied_by_scope, "P_RESTRICT_SCOPE"),
    ];
    for target in ["body", "path", "query", "headers", "args"] {
        let redact = redaction(target, "mask");
        refusals.push((format!("{redact}{SCOPE}"), "P_RESTRICT_SCOPE"));
        refusals.push((format!("{redact}{APPROVAL}"), "P_APPROVAL_REQUIRED"));
    }
    refusals.push((redaction("path", "mask"), "P_REDACT_ARGS"));
    for (controls, code) in refusals {
        let run = wrap(&controls, &line, &[]);
        run.assert_denied(code);
        assert_no_sentinel_retained(&run);
        let decision = run.decision();
        assert_eq!(decision["tool_call_id"], "req_2", "{controls}");
        assert!(decision["delegation_depth"].is_null(), "{controls}");
    }
}

#[test]
fn a_dry_run_refuses_nothing_so_a_target_elsewhere_keeps_the_call_id() {
    let line = call_line(&json!({
        "resource": "/etc/passwd",
        "_meta": {"tool_call_id": "tc-kept", "delegation": {"delegated_from": "agent-a"}},
    }));
    let controls = format!("{}{SCOPE}", redaction("body", "mask"));
    let run = wrap(&controls, &line, &["--dry-run"]);
    assert_eq!(forwarded_arguments(&run)["resource"], "/etc/passwd");
    assert_eq!(run.decision()["tool_call_id"], "tc-kept");
    assert_eq!(run.decision()["delegated_from"], "agent-a");
}

#[test]
fn a_target_elsewhere_keeps_the_call_id_and_the_delegation() {
    let line = call_line(&json!({
        "body": SENTINEL,
        "_meta": {"tool_call_id": "tc-kept", "delegation": {"delegated_from": "agent-a", "delegation_depth": 2}},
    }));
    let run = wrap(&redaction("body", "mask"), &line, &[]);
    assert_no_sentinel_retained(&run);
    let decision = run.decision();
    assert_eq!(decision["tool_call_id"], "tc-kept");
    assert_eq!(decision["delegated_from"], "agent-a");
    assert_eq!(decision["delegation_depth"], 2);
}

#[test]
fn redacted_arguments_are_not_kept_in_the_record_of_the_scope_or_approval() {
    let resource = format!("/workspace/{SENTINEL}");
    let run = wrap(
        &format!("{}{SCOPE}", redaction("args", "mask")),
        &call_line(&json!({"resource": resource, "body": "x"})),
        &[],
    );
    assert_eq!(forwarded_arguments(&run)["resource"], "[REDACTED]");
    assert_no_sentinel_retained(&run);

    let run = wrap(
        &format!("{}{APPROVAL}", redaction("args", "mask")),
        &call_line(
            &json!({"resource": SENTINEL, "_meta": {"approval": approval("deploy", SENTINEL, FAR_FUTURE)}}),
        ),
        &[],
    );
    assert_eq!(run.decision()["approval_state"], "approved");
    assert_no_sentinel_retained(&run);
}

#[test]
fn the_metadata_target_is_the_arguments_member() {
    let line = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"deploy","arguments":{"_meta":{"note":SENTINEL}},"_meta":{"progressToken":"kept"}}})
    );
    let controls = redaction("metadata", "mask");
    let run = wrap(&controls, &line, &[]);
    let frame: Value = serde_json::from_str(run.seen.trim()).unwrap();
    assert_eq!(frame["params"]["arguments"]["_meta"], "[REDACTED]");
    assert_eq!(frame["params"]["_meta"]["progressToken"], "kept");
}

#[test]
fn a_redacted_frame_changes_only_the_target() {
    let line = "{\"z\":1, \"jsonrpc\":\"2.0\",\"id\":18446744073709551617,\"method\":\"tools/call\",\"params\":{ \"count\":1e2,\"name\":\"deploy\",\"arguments\":{\"big\":18446744073709551617, \"body\":\"hunter2-sentinel\",\"esc\":\"a\\u0041\"} }}\r\n";
    let run = wrap(&redaction("body", "mask"), line, &[]);
    assert_eq!(
        run.seen,
        "{\"z\":1, \"jsonrpc\":\"2.0\",\"id\":18446744073709551617,\"method\":\"tools/call\",\"params\":{ \"count\":1e2,\"name\":\"deploy\",\"arguments\":{\"big\":18446744073709551617, \"body\":\"[REDACTED]\",\"esc\":\"a\\u0041\"} }}\r\n"
    );

    let run = wrap(&redaction("body", "drop"), line, &[]);
    let frame: Value = serde_json::from_str(run.seen.trim()).unwrap();
    assert_eq!(frame["params"]["arguments"].as_object().unwrap().len(), 2);
    assert!(
        run.seen.contains("\"big\":18446744073709551617"),
        "{}",
        run.seen
    );
    assert!(run.seen.ends_with("\r\n"), "{}", run.seen);
}

#[test]
fn an_empty_scope_value_covers_nothing() {
    for mode in ["prefix", "contains"] {
        let controls = format!(
            "  allow: [deploy]\n  restrict_scope: [deploy]\n  restrict_scope_contract:\n    scope_type: resource\n    scope_value: ''\n    scope_match_mode: {mode}\n"
        );
        let run = wrap(
            &controls,
            &call_line(&json!({"resource": "/etc/passwd"})),
            &[],
        );
        run.assert_denied("P_RESTRICT_SCOPE");
    }

    let exact = "  allow: [deploy]\n  restrict_scope: [deploy]\n  restrict_scope_contract:\n    scope_type: resource\n    scope_value: ''\n    scope_match_mode: exact\n";
    let run = wrap(exact, &call_line(&json!({"resource": ""})), &[]);
    run.assert_denied("P_RESTRICT_SCOPE");
}

#[test]
fn any_accepts_every_stated_resource_and_still_needs_one() {
    let controls = "  allow: [deploy]\n  restrict_scope: [deploy]\n  restrict_scope_contract:\n    scope_type: resource\n    scope_value: ''\n    scope_match_mode: any\n";
    let run = wrap(
        controls,
        &call_line(&json!({"resource": "/etc/passwd"})),
        &[],
    );
    assert_eq!(forwarded_arguments(&run)["resource"], "/etc/passwd");

    let run = wrap(controls, &call_line(&json!({"body": "x"})), &[]);
    run.assert_denied("P_RESTRICT_SCOPE");
}
