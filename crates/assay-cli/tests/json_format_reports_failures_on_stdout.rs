//! `--format json` puts the diagnosis on stdout when the run fails, not only when it succeeds.
//!
//! The flag documents "machine-readable report on stdout". On the failure path it wrote nothing
//! there: the diagnosis went to `summary.json` and to a human-formatted block on stderr, so a caller
//! that captures stdout and reads the exit code, which is what a non-interactive consumer has, saw
//! an empty stream from a run that had produced a complete answer including a suggested next step
//! (#2150).
//!
//! Driven rather than asserted, for the reason the gap existed at all: every constant involved was
//! already correct. `summary.json` carried `reason_code` and `next_step`, the exit code was right,
//! and the only wrong thing was which stream the bytes reached. A test over constants cannot see
//! that, which is why this one runs the binary and reads its actual stdout.

use std::io::Write;
use std::process::Command;

/// A suite that fails at load: one assertion that no trace could fail (#1949).
fn vacuous_suite(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("suite.yaml");
    let mut f = std::fs::File::create(&path).expect("create suite");
    write!(
        f,
        r#"configVersion: 1
suite: stdout_contract_probe
model: dummy
tests:
  - id: t1
    input: hello
    assertions:
      - type: trace_must_call_tool
        tool: search
        min_calls: 0
"#
    )
    .expect("write suite");
    path
}

#[test]
fn a_failing_run_writes_the_diagnosis_to_stdout_under_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let suite = vacuous_suite(dir.path());

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["run", "--format", "json", "--config"])
        .arg(&suite)
        .output()
        .expect("the binary runs");

    assert_eq!(
        out.status.code(),
        Some(2),
        "a config that cannot load is exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    assert!(
        !stdout.trim().is_empty(),
        "--format json documents a machine-readable report on stdout, and the failure path wrote \
         nothing there. stderr was: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));

    // The three fields a caller needs to act without reading a file or the human channel. Asserted
    // by name rather than by shape, because a consumer branches on `reason_code` and follows
    // `next_step`, and an object that merely parses gives it neither.
    assert_eq!(parsed["exit_code"], 2, "stdout must carry the exit code");
    assert_eq!(
        parsed["reason_code"], "E_CFG_PARSE",
        "stdout must carry the machine-readable reason"
    );
    assert!(
        parsed["next_step"]
            .as_str()
            .is_some_and(|s| s.contains("assay")),
        "a non-zero exit must suggest a concrete next command, got {:?}",
        parsed["next_step"]
    );

    // `summary.json` is unchanged and still the artifact. Consumers depend on it, and this fix adds
    // a stream rather than moving one.
    let summary = dir.path().join("summary.json");
    assert!(summary.is_file(), "summary.json must still be written");
    let from_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&summary).expect("read summary"))
            .expect("summary.json parses");
    assert_eq!(
        from_disk["reason_code"], parsed["reason_code"],
        "the artifact and stdout must be one diagnosis, not two renderings"
    );
}

#[test]
fn the_text_format_keeps_stdout_clear() {
    // The other half of the contract: `--format text` says the human report goes to stderr, so a
    // caller redirecting stdout under the default gets nothing to misparse. Without this, "put it
    // on stdout" could be satisfied by putting it there always, which would break every existing
    // pipeline that treats stdout as the machine channel only when asked.
    let dir = tempfile::tempdir().expect("tempdir");
    let suite = vacuous_suite(dir.path());

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["run", "--config"])
        .arg(&suite)
        .output()
        .expect("the binary runs");

    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "under the default format the diagnosis belongs on stderr"
    );
    assert!(
        !out.stderr.is_empty(),
        "and stderr must still carry the human report"
    );
}

#[test]
fn a_failing_ci_writes_the_diagnosis_to_stdout_under_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing.yaml");

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["ci", "--format", "json", "--config"])
        .arg(&missing)
        .output()
        .expect("the binary runs");

    assert_eq!(
        out.status.code(),
        Some(2),
        "a missing config is exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    assert!(
        !stdout.trim().is_empty(),
        "ci --format json must publish its early-failure diagnosis; stderr was: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));

    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 2);
    assert_eq!(parsed["reason_code"], "E_MISSING_CONFIG");
    assert!(
        parsed["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("assay")),
        "the diagnosis must contain actionable recovery, got {:?}",
        parsed["next_step"]
    );

    let summary = dir.path().join("summary.json");
    assert!(summary.is_file(), "summary.json must remain the artifact");
    let from_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&summary).expect("read summary"))
            .expect("summary.json parses");
    assert_eq!(
        from_disk["reason_code"], parsed["reason_code"],
        "the artifact and stdout must carry one diagnosis"
    );
}

#[test]
fn a_completed_ci_writes_its_summary_to_stdout_under_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let suite = vacuous_suite(dir.path());

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args([
            "ci",
            "--format",
            "json",
            "--allow-ineffective-assertions",
            "--config",
        ])
        .arg(&suite)
        .output()
        .expect("the binary runs");

    assert_eq!(
        out.status.code(),
        Some(1),
        "the trace-free probe completes with a failed test; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("completed ci stdout must be JSON: {e}\n{stdout}"));
    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 1);

    let from_disk: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("summary.json")).expect("read summary"),
    )
    .expect("summary.json parses");
    assert_eq!(
        parsed, from_disk,
        "stdout and summary.json must be the same authoritative report"
    );
}

#[test]
fn the_ci_text_format_keeps_stdout_clear() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing.yaml");

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["ci", "--config"])
        .arg(&missing)
        .output()
        .expect("the binary runs");

    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "ci text mode must not leak the machine report to stdout"
    );
    assert!(
        !out.stderr.is_empty(),
        "ci text mode must retain its operator diagnostic"
    );
}
