//! Opt-in stop evidence must not change protocol bytes or child exit codes.
use assert_cmd::Command;
use serde_json::Value;
use std::{fs, time::Duration};
use tempfile::TempDir;

fn wrap(d: &TempDir) -> Command {
    fs::write(
        d.path().join("policy.yaml"),
        "version: '2.0'\nname: stop-test\ntools:\n  deny: [blocked]\n",
    )
    .unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_assay"));
    c.current_dir(d.path())
        .timeout(Duration::from_secs(15))
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://stop-test",
        ]);
    c
}
fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}
fn read(d: &TempDir) -> Value {
    serde_json::from_slice(&fs::read(d.path().join("stop.json")).unwrap()).unwrap()
}

#[test]
fn stop_preserves_nonzero_exit_and_links_existing_context() {
    let d = TempDir::new().unwrap();
    wrap(&d)
        .args([
            "--session-stop-out",
            "stop.json",
            "--session-context-out",
            "context.json",
            "--",
            python(),
            "-c",
            "import sys; sys.stdin.read(); sys.exit(7)",
        ])
        .assert()
        .code(7);
    let e = read(&d);
    let ctx: Value =
        serde_json::from_slice(&fs::read(d.path().join("context.json")).unwrap()).unwrap();
    assert_eq!(e["type"], "assay.mcp.session.stop.v1");
    assert_eq!(e["data"]["session_context_id"], ctx["id"]);
    assert_eq!(e["data"]["termination"], "exited");
    assert_eq!(e["data"]["exit_code"], 7);
    assert!(e["data"]["signal"].is_null());
    assert_eq!(e["data"]["decision_log"], "absent");
    assert_eq!(e["data"]["assay_kill_requested"], false);
}
#[test]
fn stop_counts_decisions_without_entering_the_stream() {
    let d = TempDir::new().unwrap();
    wrap(&d).args(["--session-stop-out", "stop.json", "--decision-log", "decisions.jsonl", "--decision-log-new", "--", python(), "-c", "import sys; sys.stdin.read()"]).write_stdin("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"blocked\",\"arguments\":{}}}\n").assert().success();
    let e = read(&d);
    assert_eq!(e["data"]["decision_log"], "complete");
    assert_eq!(e["data"]["decision_events_written"], 1);
    assert_eq!(e["data"]["decision_events_dropped"], 0);
    let bytes = fs::read_to_string(d.path().join("decisions.jsonl")).unwrap();
    assert_eq!(bytes.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&bytes).unwrap()["type"],
        "assay.tool.decision"
    );
}
#[cfg(unix)]
#[test]
fn stop_distinguishes_signal_from_exit_one() {
    let d = TempDir::new().unwrap();
    wrap(&d)
        .args([
            "--session-stop-out",
            "stop.json",
            "--",
            python(),
            "-c",
            "import os,signal,sys; sys.stdin.read(); os.kill(os.getpid(), signal.SIGTERM)",
        ])
        .assert()
        .code(1);
    let e = read(&d);
    assert_eq!(e["data"]["termination"], "signaled");
    assert_eq!(e["data"]["signal"], 15);
    assert!(e["data"]["exit_code"].is_null());
}
#[test]
fn stop_refuses_append_log_and_existing_or_aliased_outputs_before_spawn() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("stop.json"), b"keep").unwrap();
    wrap(&d)
        .args(["--session-stop-out", "stop.json", "--", "must-not-spawn"])
        .assert()
        .failure();
    assert_eq!(fs::read(d.path().join("stop.json")).unwrap(), b"keep");
    wrap(&d)
        .args([
            "--session-stop-out",
            "other.json",
            "--decision-log",
            "decisions.jsonl",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--decision-log-new"));
    wrap(&d)
        .args([
            "--session-stop-out",
            "alias.json",
            "--session-context-out",
            "alias.json",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("different output paths"));
}

#[cfg(unix)]
#[test]
fn proxy_error_records_cleanup_and_does_not_claim_complete_log() {
    let d = TempDir::new().unwrap();
    wrap(&d).args(["--session-stop-out", "stop.json", "--decision-log", "decisions.jsonl", "--decision-log-new", "--", python(), "-c", "import sys,time; sys.stdout.buffer.write(b'\\xff\\n'); sys.stdout.flush(); time.sleep(30)"]).assert().failure();
    let e = read(&d);
    assert_eq!(e["data"]["session_end"], "proxy_error");
    assert_eq!(e["data"]["assay_kill_requested"], true);
    assert_eq!(e["data"]["termination"], "signaled");
    assert_eq!(e["data"]["decision_log"], "incomplete");
}

#[test]
fn disabled_stop_keeps_existing_protocol_and_exit_code() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let server = "import sys; sys.stdin.read(); print('final-response'); sys.exit(7)";
    let plain = wrap(&a)
        .args(["--", python(), "-c", server])
        .assert()
        .code(7)
        .get_output()
        .stdout
        .clone();
    let observed = wrap(&b)
        .args([
            "--session-stop-out",
            "stop.json",
            "--",
            python(),
            "-c",
            server,
        ])
        .assert()
        .code(7)
        .get_output()
        .stdout
        .clone();
    assert_eq!(plain, observed);
    assert!(!a.path().join("stop.json").exists());
}
