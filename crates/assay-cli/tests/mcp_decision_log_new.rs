//! `--decision-log-new` makes the decision log a file this run created; the default still appends.
use assert_cmd::Command;
use serde_json::Value;
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
        "version: '2.0'\nname: decision-log-new-test\ntools:\n  deny: [blocked_tool]\n",
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir).timeout(Duration::from_secs(15)).args([
        "mcp",
        "wrap",
        "--policy",
        "policy.yaml",
        "--event-source",
        "assay://decision-log-new-test",
    ]);
    cmd
}
fn decisions(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn new_decision_log_is_created_by_this_run_and_private() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args([
            "--decision-log",
            "decisions.jsonl",
            "--decision-log-new",
            "--",
            python(),
            "-c",
            SERVER,
        ])
        .write_stdin(REQUEST)
        .assert()
        .success();
    let events = decisions(&d.path().join("decisions.jsonl"));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "assay.tool.decision");
    assert_eq!(events[0]["data"]["decision"], "deny");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(d.path().join("decisions.jsonl"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn existing_decision_log_is_refused_before_spawn() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("decisions.jsonl"), b"keep").unwrap();
    wrap(d.path())
        .args([
            "--decision-log",
            "decisions.jsonl",
            "--decision-log-new",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("decision log"));
    assert_eq!(fs::read(d.path().join("decisions.jsonl")).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn decision_log_new_refuses_a_symlink() {
    use std::os::unix::fs::symlink;
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("keep"), b"original").unwrap();
    symlink("keep", d.path().join("decisions.jsonl")).unwrap();
    wrap(d.path())
        .args([
            "--decision-log",
            "decisions.jsonl",
            "--decision-log-new",
            "--",
            "must-not-spawn",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("decision log"));
    assert_eq!(fs::read(d.path().join("keep")).unwrap(), b"original");
}

#[test]
fn new_decision_log_cannot_alias_other_outputs() {
    // Outputs opened by path are caught by the alias check. The two reserved outputs come
    // later and refuse because their exclusive create finds the decision log already there.
    for (flag, refusal) in [
        ("--audit-log", "different output"),
        ("--coverage-out", "different output"),
        ("--state-window-out", "different output"),
        ("--tool-decision-truth-out", "different output"),
        ("--session-context-out", "cannot create new"),
        ("--request-receipts-out", "cannot create new"),
    ] {
        let d = TempDir::new().unwrap();
        fs::create_dir(d.path().join("detour")).unwrap();
        wrap(d.path())
            .args([
                "--decision-log",
                "decisions.jsonl",
                "--decision-log-new",
                flag,
                "detour/../decisions.jsonl",
                "--",
                "must-not-spawn",
            ])
            .assert()
            .failure()
            .stderr(predicates::str::contains(refusal));
    }
}

#[test]
fn decision_log_new_requires_a_decision_log() {
    let d = TempDir::new().unwrap();
    wrap(d.path())
        .args(["--decision-log-new", "--", "must-not-spawn"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--decision-log"));
}

#[test]
fn default_decision_log_still_appends() {
    let d = TempDir::new().unwrap();
    let prior = "{\"prior\":true}\n";
    fs::write(d.path().join("decisions.jsonl"), prior).unwrap();
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
    let text = fs::read_to_string(d.path().join("decisions.jsonl")).unwrap();
    assert!(text.starts_with(prior), "default must keep existing bytes");
    let events = decisions(&d.path().join("decisions.jsonl"));
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["type"], "assay.tool.decision");
}
