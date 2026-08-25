//! #2637: name a requested `--enforcement-health` artifact when execution
//! degrades to audit before the v1 producer.
//!
//! Binary contract: child is `echo MARKER` on stdout.
//!
//! Unsupported-backend (`--enforce`, no Landlock) compiles on non-Linux only.
//! Policy-conflict (`actual_enforcement` plus deny-inside-allow) compiles on
//! Linux only. That is a fail-closed precondition: on Linux without Landlock
//! the test still runs and panics because `CONFLICT_WARN` was not reached.
//! A panic is not GREEN for that callsite. No skip-return. No test-only
//! backend override. macOS/Windows CI excludes `assay-cli`.

#![cfg(unix)]

use assert_cmd::Command;
#[cfg(target_os = "linux")]
use std::io::Write;
use std::path::Path;

const MARKER: &str = "ASSAY_CHILD_RAN_MARKER";
const DEGRADE_REASON: &str = "execution degraded to audit";
const UNSUPPORTED_WARN: &str =
    "Active enforcement requested but not supported. Falling back to Audit mode.";
#[cfg(target_os = "linux")]
const CONFLICT_WARN: &str = "Landlock policy conflict detected (Deny rule inside Allow root).";

struct Run {
    code: Option<i32>,
    stderr: String,
    child_ran: bool,
    _data_home: tempfile::TempDir,
}

fn run_sandbox(extra: &[&str], policy: Option<&Path>) -> Run {
    let data_home = tempfile::tempdir().expect("temp data home");
    let mut cmd = Command::cargo_bin("assay").expect("binary");
    cmd.env("XDG_DATA_HOME", data_home.path());
    cmd.arg("sandbox");
    cmd.args(extra);
    if let Some(p) = policy {
        cmd.arg("--policy").arg(p);
    }
    cmd.args(["--", "echo", MARKER]);
    let out = cmd.assert().get_output().clone();
    Run {
        code: out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        child_ran: String::from_utf8_lossy(&out.stdout).contains(MARKER),
        _data_home: data_home,
    }
}

fn assert_continuing(run: &Run, path_warn: &str) {
    assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
    assert!(run.child_ran, "child must run.\nstderr:\n{}", run.stderr);
    assert!(
        run.stderr.contains(path_warn),
        "this callsite was not reached.\nstderr:\n{}",
        run.stderr
    );
    for flag in ["--profile", "--bundle", "--otel-jsonl"] {
        assert!(
            !run.stderr.contains(&format!("NOTE: {flag} ")),
            "must not claim {flag} unwritten on a continuing run.\nstderr:\n{}",
            run.stderr
        );
    }
}

fn assert_named_health(run: &Run, health: &Path, path_warn: &str) {
    assert_continuing(run, path_warn);
    let note = format!(
        "NOTE: --enforcement-health {} not written: {DEGRADE_REASON}",
        health.display()
    );
    assert!(
        run.stderr.contains(&note),
        "missing health NOTE.\nstderr:\n{}",
        run.stderr
    );
    assert!(!health.exists(), "must not synthesize a v1 artifact");
}

fn assert_silent_health(run: &Run, path_warn: &str) {
    assert_continuing(run, path_warn);
    assert!(
        !run.stderr.contains("--enforcement-health") && !run.stderr.contains(DEGRADE_REASON),
        "no health path requested.\nstderr:\n{}",
        run.stderr
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn unsupported_backend_names_requested_enforcement_health_as_unwritten() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let health = tmp.path().join("health.json");
    let profile = tmp.path().join("prof.yaml");
    let hs = health.to_string_lossy();
    let ps = profile.to_string_lossy();
    let run = run_sandbox(
        &[
            "--enforce",
            "--enforce-net",
            "--enforcement-health",
            hs.as_ref(),
            "--profile",
            ps.as_ref(),
        ],
        None,
    );
    assert_named_health(&run, &health, UNSUPPORTED_WARN);
    assert!(
        profile.exists(),
        "profile must still be written.\nstderr:\n{}",
        run.stderr
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn no_enforcement_health_path_stays_silent_on_unsupported_backend_degrade() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let profile = tmp.path().join("prof.yaml");
    let ps = profile.to_string_lossy();
    let run = run_sandbox(
        &["--enforce", "--enforce-net", "--profile", ps.as_ref()],
        None,
    );
    assert_silent_health(&run, UNSUPPORTED_WARN);
    assert!(profile.exists(), "stderr:\n{}", run.stderr);
}

#[cfg(target_os = "linux")]
fn conflict_policy() -> tempfile::NamedTempFile {
    let home = std::env::var("HOME").expect("HOME");
    let deny = std::path::PathBuf::from(&home).join(".ssh");
    let mut f = tempfile::NamedTempFile::new().expect("temp policy");
    writeln!(
        f,
        "api_version: assay/v1\nfs:\n  allow:\n    - {home}\n  deny:\n    - {}\nnet:\n  allow: []\n  deny: []",
        deny.display()
    )
    .expect("write");
    f.flush().expect("flush");
    f
}

#[cfg(target_os = "linux")]
fn run_conflict(with_health: bool) -> (Run, tempfile::TempDir, tempfile::NamedTempFile) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let health = tmp.path().join("health.json");
    let profile = tmp.path().join("prof.yaml");
    let policy = conflict_policy();
    let hs = health.to_string_lossy();
    let ps = profile.to_string_lossy();
    let mut extra = vec!["--enforce", "--enforce-net", "--profile", ps.as_ref()];
    if with_health {
        extra.extend(["--enforcement-health", hs.as_ref()]);
    }
    let run = run_sandbox(&extra, Some(policy.path()));
    if run.stderr.contains(UNSUPPORTED_WARN) {
        panic!(
            "policy-conflict callsite was not reached (no Landlock). \
             This is not GREEN for that branch.\nstderr:\n{}",
            run.stderr
        );
    }
    (run, tmp, policy)
}

#[cfg(target_os = "linux")]
#[test]
fn policy_conflict_names_requested_enforcement_health_as_unwritten() {
    let (run, tmp, _policy) = run_conflict(true);
    let health = tmp.path().join("health.json");
    let profile = tmp.path().join("prof.yaml");
    assert_named_health(&run, &health, CONFLICT_WARN);
    assert!(profile.exists(), "stderr:\n{}", run.stderr);
}

#[cfg(target_os = "linux")]
#[test]
fn no_enforcement_health_path_stays_silent_on_policy_conflict_degrade() {
    let (run, tmp, _policy) = run_conflict(false);
    let profile = tmp.path().join("prof.yaml");
    assert_silent_health(&run, CONFLICT_WARN);
    assert!(profile.exists(), "stderr:\n{}", run.stderr);
}
