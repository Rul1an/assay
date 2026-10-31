//! Session context is an opt-in companion, never a decision-log record.
use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
use tempfile::TempDir;

const SERVER: &str = "import sys,json\nfor line in sys.stdin:\n r=json.loads(line); print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'ok':True}}),flush=True)\n";
const REQUEST: &str = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"blocked_tool\",\"arguments\":{}}}\n";
fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}
fn wrap(dir: &Path) -> Command {
    fs::write(
        dir.join("policy.yaml"),
        "version: '2.0'\nname: context-test\ntools:\n  deny: [blocked_tool]\n",
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir).timeout(Duration::from_secs(15)).args([
        "mcp",
        "wrap",
        "--policy",
        "policy.yaml",
        "--event-source",
        "assay://context-test",
        "--label",
        "same-label",
    ]);
    cmd
}
fn run_context(dir: &Path, dry_run: bool, extra: &str) -> Value {
    let mut cmd = wrap(dir);
    cmd.args([
        "--session-context-out",
        "context.json",
        "--decision-log",
        "decisions.jsonl",
    ]);
    if dry_run {
        cmd.arg("--dry-run");
    }
    cmd.args(["--", python(), "-c", SERVER, extra])
        .write_stdin(REQUEST)
        .assert()
        .success();
    let bytes = fs::read(dir.join("context.json")).unwrap();
    let event: Value = serde_json::from_slice(&bytes).unwrap();
    fn assert_no_plaintext(value: &Value, secret: &str) {
        match value {
            Value::String(text) => assert!(!text.contains(secret), "plaintext argv leaked"),
            Value::Array(values) => values.iter().for_each(|v| assert_no_plaintext(v, secret)),
            Value::Object(values) => values.iter().for_each(|(key, v)| {
                assert!(!key.contains(secret), "plaintext argv leaked in key");
                assert_no_plaintext(v, secret);
            }),
            _ => {}
        }
    }
    assert_no_plaintext(&event, extra);
    assert_no_plaintext(&event, SERVER);
    assert_eq!(event["type"], "assay.mcp.session.context.v1");
    assert_eq!(event["specversion"], "1.0");
    assert_eq!(event["source"], "assay://context-test");
    assert!(event["id"].as_str().unwrap().len() > 10);
    assert_eq!(event["data"]["server_id"], "same-label");
    assert_eq!(
        event["data"]["enforcement"],
        if dry_run { "dry_run" } else { "enforce" }
    );
    assert_eq!(event["data"]["server_binding_basis"], "spawned_command");
    assert_eq!(event["data"]["server_binding_digest_alg"], "sha256");
    assert_eq!(
        event["data"]["server_binding_canonicalization"],
        "jcs:argv.v1"
    );
    // A string array has no object keys or numeric normalization. This independent
    // encoding intentionally does not call Assay's JCS/digest helper.
    let expected = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&[python(), "-c", SERVER, extra]).unwrap()
        ))
    );
    assert_eq!(event["data"]["server_binding_digest"], expected);
    let evidence = assay_evidence::types::EvidenceEvent::new(
        "assay.mcp.session.context.v1",
        "assay://context-test",
        "context-conformance",
        0,
        event["data"].clone(),
    );
    let mut bundle = Vec::new();
    let mut writer = assay_evidence::bundle::writer::BundleWriter::new(&mut bundle);
    writer.add_event(evidence);
    writer.finish().unwrap();
    assert_eq!(
        assay_evidence::bundle::writer::verify_bundle(std::io::Cursor::new(bundle))
            .unwrap()
            .event_count,
        1
    );
    let decisions: Vec<Value> = fs::read_to_string(dir.join("decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(decisions.len(), 1, "context must not enter decision stream");
    assert_eq!(decisions[0]["type"], "assay.tool.decision");
    assert_eq!(
        decisions[0]["data"]["decision"],
        if dry_run { "allow" } else { "deny" }
    );
    event
}
#[test]
fn context_distinguishes_mode_and_argv_without_changing_decisions() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let c = TempDir::new().unwrap();
    let x = run_context(a.path(), false, "secret-A-雪-\"quote\"");
    let y = run_context(b.path(), true, "secret-A-雪-\"quote\"");
    let z = run_context(c.path(), true, "secret-B-雪-\"quote\"");
    assert_eq!(
        x["data"]["server_binding_digest"],
        y["data"]["server_binding_digest"]
    );
    assert_ne!(
        y["data"]["server_binding_digest"],
        z["data"]["server_binding_digest"]
    );
}
#[test]
fn coverage_without_explicit_decision_log_keeps_working_with_context() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "context.json",
            "--coverage-out",
            "coverage.json",
            "--",
            python(),
            "-c",
            SERVER,
        ])
        .write_stdin(REQUEST)
        .assert()
        .success();
    assert!(d.path().join("context.json").is_file());
    let coverage: Value =
        serde_json::from_slice(&fs::read(d.path().join("coverage.json")).unwrap()).unwrap();
    assert_eq!(
        coverage["tools"]["tools_seen"],
        serde_json::json!(["blocked_tool"])
    );
}
#[test]
fn existing_context_is_not_overwritten() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("context.json"), b"keep").unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "context.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("session context"));
    assert_eq!(fs::read(d.path().join("context.json")).unwrap(), b"keep");
}
#[test]
fn context_cannot_alias_other_outputs() {
    for flag in [
        "--decision-log",
        "--audit-log",
        "--coverage-out",
        "--state-window-out",
        "--tool-decision-truth-out",
    ] {
        let d = TempDir::new().unwrap();
        fs::create_dir(d.path().join("detour")).unwrap();
        wrap(d.path())
            .args([
                "--session-context-out",
                "context.json",
                flag,
                "detour/../context.json",
                "--",
                "must-not-spawn",
            ])
            .assert()
            .failure()
            .stderr(predicates::str::contains("different output"));
    }
}
#[cfg(unix)]
#[test]
fn context_rejects_symlinks_and_parent_aliases() {
    use std::os::unix::fs::symlink;
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("keep"), b"original").unwrap();
    symlink("keep", d.path().join("context.json")).unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "context.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("session context"));
    assert_eq!(fs::read(d.path().join("keep")).unwrap(), b"original");
    symlink(d.path(), d.path().join("alias")).unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "new.json",
            "--decision-log",
            "alias/new.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("different output"));
}

#[test]
fn context_is_opt_in_and_spawn_failure_never_leaves_a_valid_record() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args([
            "--decision-log",
            "decisions.jsonl",
            "--",
            python(),
            "-c",
            SERVER,
        ])
        .write_stdin(REQUEST)
        .assert()
        .success();
    assert!(!d.path().join("context.json").exists());
    let rows: Vec<Value> = fs::read_to_string(d.path().join("decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["type"], "assay.tool.decision");
    wrap(d.path())
        .args([
            "--session-context-out",
            "context.json",
            "--",
            "must-not-exist-assay-fixture",
        ])
        .assert()
        .failure();
    assert!(
        !d.path().join("context.json").exists(),
        "failed spawn must not leave startup context or consume its path"
    );
}

#[test]
fn unavailable_context_directory_fails_before_spawn() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "missing/context.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "cannot create new session context file",
        ));
}

#[test]
fn context_can_be_the_only_output_and_has_validated_source() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args([
            "--session-context-out",
            "context.json",
            "--",
            python(),
            "-c",
            SERVER,
        ])
        .write_stdin(REQUEST)
        .assert()
        .success();
    let event: Value =
        serde_json::from_slice(&fs::read(d.path().join("context.json")).unwrap()).unwrap();
    assert_eq!(event["datacontenttype"], "application/json");
    assert_eq!(event["source"], "assay://context-test");
    chrono::DateTime::parse_from_rfc3339(event["time"].as_str().unwrap()).unwrap();
    assert!(!d.path().join("decisions.jsonl").exists());
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(d.path())
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "not-a-uri",
            "--session-context-out",
            "invalid.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "event_source must be absolute URI",
        ));
    assert!(!d.path().join("invalid.json").exists());
}

#[test]
fn context_rejects_blank_source_before_reserving_output_or_starting_child() {
    for source in ["assay://context-test", "", " \t\n"] {
        let d = TempDir::new().unwrap();
        fs::write(
            d.path().join("policy.yaml"),
            "version: '2.0'\nname: context-test\ntools:\n  deny: [blocked_tool]\n",
        )
        .unwrap();
        let child =
            format!("from pathlib import Path\nPath('started').write_text('started')\n{SERVER}");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
        cmd.current_dir(d.path())
            .timeout(Duration::from_secs(15))
            .args([
                "mcp",
                "wrap",
                "--policy",
                "policy.yaml",
                "--event-source",
                source,
                "--session-context-out",
                "context.json",
                "--",
                python(),
                "-c",
                &child,
            ])
            .write_stdin(REQUEST);
        if source == "assay://context-test" {
            cmd.assert().success();
            assert!(
                d.path().join("started").exists(),
                "positive child-start control"
            );
            assert!(d.path().join("context.json").exists());
        } else {
            let result = cmd.assert().failure();
            let stderr = String::from_utf8_lossy(&result.get_output().stderr);
            assert!(stderr.contains("session context requires event source"));
            assert!(
                !stderr.contains("[assay] wrapping command:"),
                "invalid source reached spawn boundary"
            );
            assert!(!d.path().join("started").exists());
            assert!(
                !d.path().join("context.json").exists(),
                "invalid source reserved output"
            );
        }
    }
}
