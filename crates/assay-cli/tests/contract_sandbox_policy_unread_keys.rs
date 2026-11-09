//! A sandbox policy file holds only the keys `assay sandbox --policy` reads.
//!
//! The loader reads `api_version`, `extends`, `fs` (`allow`, `deny`) and `net` (`allow`, `deny`).
//! It used to ignore every other key, so a misspelled key such as `fs: {denny: [...]}` or a wrong
//! root key such as `filesystem:` loaded as a policy with no rules and the command ran, and the
//! `env`, `processes` and `meta` blocks that `assay sandbox --profile` wrote into its suggestion
//! read as policy while nothing acted on them: a command missing from `processes.allow` ran.
//!
//! A named policy that holds a key the loader does not read is now refused like any other policy
//! that does not load (ADR-043 §5): exit 2, `E_POLICY_LOAD_FAILED_UNENFORCEABLE`, and the child
//! never runs. As in `contract_sandbox_policy_load_fail_closed.rs`, the child is `echo <marker>`,
//! so "did it run?" is read from stdout rather than inferred from the exit code, and the controls
//! require the marker so a rule that refused everything could not pass.

// `echo` is POSIX; the neighbouring sandbox contract tests gate themselves the same way.
#![cfg(unix)]

use assert_cmd::Command;
use std::path::Path;

const MARKER: &str = "ASSAY_CHILD_RAN_MARKER";
const REASON: &str = "E_POLICY_LOAD_FAILED_UNENFORCEABLE";

struct Run {
    code: Option<i32>,
    stderr: String,
    child_ran: bool,
}

fn sandbox(before: &[&str], extra: &[&str], envs: &[(&str, &str)], policy: Option<&Path>) -> Run {
    // The refusal path increments a counter in the metrics store, which resolves its location
    // from XDG_DATA_HOME; keep the suite out of the developer's real store.
    let data_home = tempfile::tempdir().expect("temp data home");
    let mut cmd = Command::cargo_bin("assay").expect("binary");
    cmd.env("XDG_DATA_HOME", data_home.path());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.args(before);
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
    }
}

fn policy_file(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write policy");
    path
}

fn assert_refused(run: &Run, case: &str) {
    assert_eq!(
        run.code,
        Some(2),
        "{case}: a key the loader does not read must end the run.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.child_ran,
        "{case}: the child must not run.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains(REASON),
        "{case}: the refusal carries the policy-load reason code.\nstderr:\n{}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("mcp-server-minimal"),
        "{case}: no substitute policy may be applied.\nstderr:\n{}",
        run.stderr
    );
}

/// The suggestion `assay sandbox --profile` wrote up to and including 6.23.0, frozen.
const SUGGESTION_UP_TO_6_23_0: &str = include_str!("fixtures/sandbox/suggestion_v6_23_0.yaml");

#[test]
fn every_key_the_loader_does_not_read_is_refused_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rows: &[(&str, &str, &[&str])] = &[
        (
            "misspelled fs key",
            "api_version: assay/v1\nfs:\n  denny: [\"/etc\"]\n",
            &["\"fs.denny\""],
        ),
        (
            "unknown net key",
            "net:\n  permit: [\"443\"]\n",
            &["\"net.permit\""],
        ),
        (
            "wrong root key",
            "filesystem:\n  deny: [\"/etc\"]\n",
            &["\"filesystem\""],
        ),
        (
            "processes",
            "processes:\n  allow: [\"/bin/ls\"]\n",
            &["\"processes\""],
        ),
        ("env", "env:\n  allow: [\"X\"]\n", &["\"env\""]),
        ("meta", "meta:\n  notes: []\n", &["\"meta\""]),
        ("null-valued key", "processes:\n", &["\"processes\""]),
        ("non-string key", "1: a\n", &["\"1\""]),
        (
            "suggestion written up to 6.23.0",
            SUGGESTION_UP_TO_6_23_0,
            &["\"env\"", "\"meta\"", "\"processes\""],
        ),
        (
            "MCP policy shape",
            "version: \"1.0\"\nname: shared\ntools:\n  deny: [rm]\nsequences: []\n",
            &["\"name\"", "\"sequences\"", "\"tools\"", "\"version\""],
        ),
        (
            "assay policy generate shape",
            "_meta:\n  name: x\nfiles:\n  allow: []\nnetwork:\n  allow: []\nprocesses:\n  allow: []\n",
            &["\"_meta\"", "\"files\"", "\"network\"", "\"processes\""],
        ),
    ];
    for (i, (case, body, named)) in rows.iter().enumerate() {
        let path = policy_file(dir.path(), &format!("p{i}.yaml"), body);
        let run = sandbox(&[], &[], &[], Some(&path));
        assert_refused(&run, case);
        for key in *named {
            assert!(
                run.stderr.contains(key),
                "{case}: the refusal names {key}.\nstderr:\n{}",
                run.stderr
            );
        }
    }
}

/// The blocks `--profile` used to write are named with a way out, since every suggestion written
/// before this change has all three.
#[test]
fn an_earlier_suggestion_is_refused_with_its_migration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = policy_file(dir.path(), "old.yaml", SUGGESTION_UP_TO_6_23_0);
    let run = sandbox(&[], &[], &[], Some(&path));
    assert_refused(&run, "suggestion written up to 6.23.0");
    assert!(
        run.stderr.contains("delete those blocks"),
        "the refusal says how to keep using the file.\nstderr:\n{}",
        run.stderr
    );
}

/// A key is a user string that reaches the terminal in the refusal; a control byte in it must be
/// escaped, not written.
#[test]
fn a_key_with_a_control_byte_is_named_escaped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = policy_file(dir.path(), "esc.yaml", "\"evil\\e[2J\": 1\n");
    let run = sandbox(&[], &[], &[], Some(&path));
    assert_refused(&run, "control byte in a key");
    assert!(
        !run.stderr.contains('\u{1b}'),
        "no raw escape byte may reach the terminal.\nstderr:\n{:?}",
        run.stderr
    );
}

/// `--quiet` hides the banner, not a refusal.
#[test]
fn quiet_does_not_hide_the_refusal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = policy_file(dir.path(), "q.yaml", "fs:\n  denny: [\"/etc\"]\n");
    let runs = [
        (
            "sandbox --quiet",
            sandbox(&[], &["--quiet"], &[], Some(&path)),
        ),
        (
            "assay --quiet",
            sandbox(&["--quiet"], &[], &[], Some(&path)),
        ),
        (
            "ASSAY_QUIET=1",
            sandbox(&[], &[], &[("ASSAY_QUIET", "1")], Some(&path)),
        ),
    ];
    for (case, run) in &runs {
        assert_refused(run, case);
        assert!(
            run.stderr.contains("\"fs.denny\""),
            "{case}: the refusal names the key.\nstderr:\n{}",
            run.stderr
        );
    }
}

/// An unread key is named even when the file also asks for `extends`, which is refused on its own.
#[test]
fn an_unread_key_is_named_before_the_extends_refusal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = policy_file(
        dir.path(),
        "both.yaml",
        "extends: [\"pack:mcp-server-minimal\"]\nexec_allow: {}\n",
    );
    let run = sandbox(&[], &[], &[], Some(&path));
    assert_refused(&run, "unread key beside extends");
    assert!(
        run.stderr.contains("\"exec_allow\""),
        "the unread key is named.\nstderr:\n{}",
        run.stderr
    );
}

/// What `--profile` writes loads back under `--policy` and the command runs. Before this change
/// that held only because the loader ignored the blocks it did not read.
#[test]
fn a_generated_suggestion_loads_back_and_the_command_runs() {
    for format in ["yaml", "json"] {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join(format!("suggestion.{format}"));
        let data_home = tempfile::tempdir().expect("temp data home");
        let mut cmd = Command::cargo_bin("assay").expect("binary");
        cmd.env("XDG_DATA_HOME", data_home.path())
            .arg("sandbox")
            .arg("--profile")
            .arg(&out)
            .arg("--profile-format")
            .arg(format)
            .arg("--quiet")
            .args(["--", "true"]);
        cmd.assert().success();

        let run = sandbox(&[], &[], &[], Some(&out));
        assert!(
            !run.stderr.contains(REASON),
            "{format}: the generated suggestion must load.\nstderr:\n{}",
            run.stderr
        );
        assert_eq!(run.code, Some(0), "{format}: stderr:\n{}", run.stderr);
        assert!(run.child_ran, "{format}: stderr:\n{}", run.stderr);
    }
}

/// The schema example on `docs/reference/sandbox-policies.md`, and the other shapes that loaded
/// before, still load.
#[test]
fn documented_and_existing_shapes_still_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shapes = [
        (
            "current schema example",
            "api_version: \"assay/v1\"\n\nfs:\n  allow:\n    - \"/opt/example-data\"\n  deny:\n    - \"${HOME}/.ssh\"\n\nnet:\n  allow: []\n  deny: []\n",
        ),
        ("json fs only", "{\"fs\": {\"allow\": [], \"deny\": []}}"),
        ("null known key", "fs:\n  allow:\n"),
        ("integer api_version", "api_version: 1\n"),
        ("net only", "net:\n  allow: []\n"),
    ];
    for (i, (case, body)) in shapes.iter().enumerate() {
        let path = policy_file(dir.path(), &format!("ok{i}.yaml"), body);
        let run = sandbox(&[], &[], &[], Some(&path));
        assert!(
            !run.stderr.contains(REASON),
            "{case}: must load.\nstderr:\n{}",
            run.stderr
        );
    }
}
