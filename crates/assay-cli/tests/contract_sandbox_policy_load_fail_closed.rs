//! ADR-043 §5: a named policy that cannot be loaded is fatal, unconditionally.
//!
//! `assay sandbox --policy <path>` used to fall back to the built-in `mcp-server-minimal`
//! policy whenever the named file failed to load, including under `--fail-closed`: the child
//! ran to completion under containment the operator never chose, and exit 0 reported success.
//! Naming `--policy` is an instruction. Running something else does not carry it out, and
//! `--fail-closed` is not what creates that obligation, it only makes ignoring it more
//! obviously wrong.
//!
//! These are contract tests over the shipped binary rather than unit tests over a helper,
//! because the defect was in the wiring and not in any single function.
//!
//! The child is `echo <marker>` rather than `true`, so the claim under test is checked
//! directly: an exit code proves the CLI refused, but only an absent marker proves nothing was
//! executed. The control arm inverts it and requires the marker, so a rule that simply refused
//! everything could not pass the suite either.
//!
//! The marker travels on stdout rather than as a file. A filesystem marker looked equivalent
//! and was not: on Linux the sandbox runs Landlock in containment, and `mcp-server-minimal`
//! correctly denies writing into the outer `/tmp`, so the child ran and still left no file.
//! Asserting on a channel the policy under test governs would have made the control arm a
//! test of the default policy's deny rules instead of a test of the load-failure path.

// `echo` is POSIX, and the neighbouring sandbox integration test gates itself the same way.
// CI does not catch this on its own: the non-Linux matrix legs run
// `cargo test --workspace --exclude assay-cli`, so a Windows break here would stay invisible
// until that exclusion changes.
#![cfg(unix)]

use assert_cmd::Command;
use std::io::Write;
use std::path::Path;

const MARKER: &str = "ASSAY_CHILD_RAN_MARKER";

fn broken_policy() -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("temp policy");
    // Well-formed enough to be opened, not well-formed enough to parse as a policy.
    writeln!(f, "this: is: not: a: valid: policy: document").expect("write");
    f.flush().expect("flush");
    f
}

fn policy_with_unresolved_extends() -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("temp policy");
    writeln!(
        f,
        "api_version: assay/v1\nextends:\n  - pack:mcp-server-minimal\nfs:\n  allow: []\n  deny: []\nnet:\n  allow: []\n  deny: []"
    )
    .expect("write");
    f.flush().expect("flush");
    f
}

/// Outcome of one sandbox invocation over a child that announces itself on stdout, so "did it
/// execute?" is an observable fact rather than an inference from the exit code.
struct Run {
    code: Option<i32>,
    stderr: String,
    child_ran: bool,
    _data_home: tempfile::TempDir,
}

fn run_sandbox(extra: &[&str], policy: Option<&Path>) -> Run {
    // The refusal path increments a counter, and the metrics store resolves its location from
    // XDG_DATA_HOME with a fallback to ~/.local/share/assay. Without this the suite writes into
    // the developer's and the runner's real metrics file, so a test run silently mutates state
    // it does not own.
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

#[test]
fn a_named_policy_that_does_not_load_is_fatal_without_any_flag() {
    let policy = broken_policy();
    let run = run_sandbox(&[], Some(policy.path()));

    assert_eq!(
        run.code,
        Some(2),
        "naming --policy is an instruction; a load failure must end the run.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.child_ran,
        "the child must never be executed when the named policy did not load.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("E_POLICY_LOAD_FAILED_UNENFORCEABLE"),
        "refusal must carry its reason code.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("mcp-server-minimal"),
        "no substitute policy may be applied for a named policy.\nstderr:\n{}",
        run.stderr
    );
}

#[test]
fn fail_closed_does_not_change_the_outcome_it_only_agrees_with_it() {
    let policy = broken_policy();
    let bare = run_sandbox(&[], Some(policy.path()));
    let flagged = run_sandbox(&["--fail-closed"], Some(policy.path()));

    assert_eq!(
        flagged.code,
        Some(2),
        "--fail-closed must not be weaker than the default.\nstderr:\n{}",
        flagged.stderr
    );
    assert!(
        !flagged.child_ran,
        "the child must never be executed under --fail-closed either.\nstderr:\n{}",
        flagged.stderr
    );
    assert_eq!(
        bare.code, flagged.code,
        "the obligation comes from naming --policy, not from --fail-closed"
    );
}

#[test]
fn a_named_policy_with_unresolved_extends_is_fatal_before_execution() {
    let policy = policy_with_unresolved_extends();
    let run = run_sandbox(&[], Some(policy.path()));

    assert_eq!(
        run.code,
        Some(2),
        "an unsupported policy composition request must be refused.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.child_ran,
        "the child must not run after a declared policy input was ignored.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("E_POLICY_LOAD_FAILED_UNENFORCEABLE"),
        "the refusal must use the existing policy-load reason code.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("does not support non-empty `extends`"),
        "the operator must receive an actionable, value-free explanation.\nstderr:\n{}",
        run.stderr
    );
}

#[test]
fn the_documented_default_still_applies_when_no_policy_is_named() {
    // Substitution stays legitimate exactly where the operator named nothing. This is the
    // control arm: a rule that refused here, or that never executed anything, would satisfy
    // the two tests above while having broken the documented default.
    let run = run_sandbox(&[], None);

    // Pinned to exactly 0, not merely "not 2": a control arm that accepts any other failure
    // would read green while the default path was broken in some unrelated way.
    assert_eq!(
        run.code,
        Some(0),
        "no --policy was named, so the documented default must still apply and the run must \
         succeed.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.child_ran,
        "the default path must actually execute the child.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("E_POLICY_LOAD_FAILED_UNENFORCEABLE"),
        "the default path is not a load failure.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("mcp-server-minimal"),
        "the default path must still name the policy it applied.\nstderr:\n{}",
        run.stderr
    );
}

#[test]
fn a_refusal_states_which_requested_artifacts_were_not_written() {
    // `write_enforcement_health_v1` calls a requested artifact that cannot be written an
    // error, "so the caller does not exit successfully in a state where the evidence is
    // absent on disk". A refusal exits before `maybe_profile_finish`, the only place
    // artifacts are written, so the file is absent either way. The caller is owed the
    // sentence rather than an empty path to discover later.
    let policy = broken_policy();
    let tmp = tempfile::tempdir().expect("temp dir");
    let profile = tmp.path().join("prof.yaml");
    let bundle = tmp.path().join("bundle.tar.gz");

    let out = Command::cargo_bin("assay")
        .expect("binary")
        .env("XDG_DATA_HOME", tmp.path())
        .args(["sandbox", "--policy"])
        .arg(policy.path())
        .arg("--profile")
        .arg(&profile)
        .arg("--bundle")
        .arg(&bundle)
        .args(["--", "echo", MARKER])
        .assert()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        !profile.exists() && !bundle.exists(),
        "a refused run must not leave artifacts that look like a measured run"
    );
    for flag in ["--profile", "--bundle"] {
        assert!(
            stderr.contains(flag) && stderr.contains("not written"),
            "refusal must state that {flag} was not written.\nstderr:\n{stderr}"
        );
    }
}
