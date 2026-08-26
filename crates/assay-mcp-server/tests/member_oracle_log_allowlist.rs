//! Bite tests for the member-oracle log-name allowlist.
//!
//! Env-controlled log paths must be closed basenames. A separator, `..`, or an
//! unknown name is refused before `open`. Abstract fixture only.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn oracle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/member_oracles.py")
}

fn check_log_name(name: &str) -> std::process::Output {
    Command::new(python())
        .args([oracle_path().to_str().unwrap(), "check-log-name", name])
        .output()
        .expect("spawn oracle check-log-name")
}

#[test]
fn allowed_basenames_are_admitted() {
    for name in ["raw.log", "interpret.ndjson"] {
        let out = check_log_name(name);
        assert!(
            out.status.success(),
            "{name} must be admitted: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn separator_is_refused_before_open() {
    for name in ["foo/bar", r"foo\bar", "/tmp/raw.log"] {
        let out = check_log_name(name);
        assert!(!out.status.success(), "{name:?} must fail before open");
    }
}

#[test]
fn parent_component_is_refused_before_open() {
    let out = check_log_name("..");
    assert!(!out.status.success(), ".. must fail before open");
    let out = check_log_name("../escape.log");
    assert!(!out.status.success(), "../escape.log must fail before open");
}

#[test]
fn unknown_basename_is_refused_before_open() {
    let out = check_log_name("unknown.log");
    assert!(!out.status.success(), "unknown.log must fail before open");
}

#[test]
fn serve_does_not_open_a_rejected_name() {
    let dir = tempfile::tempdir().unwrap();
    let token = "oracle-allowlist-escape-must-not-exist.log";
    let outside = dir.path().parent().expect("temp parent").join(token);
    let _ = std::fs::remove_file(&outside);
    let mut child = Command::new(python())
        .args([oracle_path().to_str().unwrap(), "serve", "last"])
        .current_dir(dir.path())
        .env("ORACLE_RAW_LOG", format!("../{token}"))
        .env("ORACLE_INTERPRET_LOG", "interpret.ndjson")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn oracle serve");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let _ = stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    }
    let output = child.wait_with_output().expect("oracle exit");
    assert!(
        !outside.exists(),
        "rejected name must not be opened; {token} was created"
    );
    assert!(
        !output.status.success(),
        "serve must refuse a rejected log name at startup"
    );
}
