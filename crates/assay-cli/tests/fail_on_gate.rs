//! `--fail-on` decides which findings gate a lint run. An unrecognized value must not decide it.
//!
//! The value this guards against is not hypothetical: `assay-action` passed `--fail-on none` for as
//! long as `none` had no arm, and the fallback silently gated on error findings — the opposite of
//! what the caller asked for. These tests assert at the process boundary, because that is where the
//! Action reads the answer.

use std::path::PathBuf;
use std::process::{Command, Output};

/// A verified bundle with no findings, so every valid threshold exits 0 on it. That is what makes
/// it useful here: any non-zero exit in these tests comes from the argument, not the evidence.
fn fixture_bundle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/evidence/test-bundle.tar.gz")
}

fn lint_with(fail_on: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["evidence", "lint"])
        .arg(fixture_bundle())
        .args(["--format", "json", "--fail-on", fail_on])
        .output()
        .expect("failed to run assay")
}

/// The fixture must actually be reachable and clean, or the tests below prove nothing: a missing
/// file also produces a non-zero exit, and would make the rejection tests pass for the wrong reason.
#[test]
fn the_fixture_exits_zero_so_a_failure_below_means_the_argument() {
    assert!(
        fixture_bundle().exists(),
        "fixture missing at {}",
        fixture_bundle().display()
    );
    let out = lint_with("error");
    assert_eq!(
        out.status.code(),
        Some(0),
        "fixture no longer lints clean: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The acceptance criterion from #2032: a run that would otherwise have exited 0 must not exit 0
/// when the threshold it was given does not exist. Before the value parser, `--fail-on nope` was
/// accepted and silently became `error`.
#[test]
fn a_run_that_would_pass_does_not_pass_with_an_unrecognized_threshold() {
    let out = lint_with("nope");
    assert_ne!(
        out.status.code(),
        Some(0),
        "an unrecognized --fail-on was accepted"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("nope"),
        "the error does not name the rejected value: {stderr}"
    );
    assert!(
        stderr.contains("possible values"),
        "the error does not name the accepted set: {stderr}"
    );
    assert!(
        out.stdout.is_empty(),
        "a rejected run still wrote to stdout, where the report would go"
    );
}

/// `warnings` is the plural of a real value and the most likely typo. It must not be treated as
/// `warn`, and must not silently become `error` either.
#[test]
fn a_near_miss_spelling_is_rejected_rather_than_guessed() {
    let out = lint_with("warnings");
    assert_ne!(out.status.code(), Some(0), "`warnings` was accepted");
}

/// Every spelling the argument advertises must be usable. `warning` is an accepted alias of `warn`
/// that the help text does not spell out, and `none` is the value `assay-action` depends on.
#[test]
fn every_advertised_spelling_is_accepted() {
    for value in ["error", "warn", "warning", "info", "none"] {
        let out = lint_with(value);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("invalid value"),
            "`--fail-on {value}` is advertised but rejected: {stderr}"
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "`--fail-on {value}` gated a bundle with no findings: {stderr}"
        );
    }
}
