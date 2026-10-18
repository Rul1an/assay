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

fn lint_pack(pack: &str, fail_on: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["evidence", "lint"])
        .arg(fixture_bundle())
        .args(["--pack", pack, "--format", "json", "--fail-on", fail_on])
        .output()
        .expect("failed to run assay")
}

/// The same fixture carries findings under other packs — one Warn under `eu-ai-act-baseline`, one
/// Error under `soc2-baseline`. That is what lets these tests compare thresholds by behaviour
/// rather than by acceptance, which a bundle with no findings cannot do.
const WARNING_PACK: &str = "eu-ai-act-baseline";

/// The pack that yields an Error finding on the same fixture. `none` reinterpreted as `Error` is
/// invisible on a warning bundle — that threshold fires only on errors — so this is the only pack
/// here that can tell the two apart.
const ERROR_PACK: &str = "soc2-baseline";

/// The defect itself: `--fail-on none` must not gate an error finding. Restoring
/// `"none" => Some(Severity::Error)` makes exactly this assertion fail, and no other in this file.
#[test]
fn none_does_not_gate_an_error_finding() {
    assert_eq!(
        lint_pack(ERROR_PACK, "error").status.code(),
        Some(1),
        "fixture no longer yields an error finding under {ERROR_PACK}, so the next assertion proves nothing"
    );
    assert_eq!(
        lint_pack(ERROR_PACK, "none").status.code(),
        Some(0),
        "--fail-on none gated an error finding, which is the defect this file exists for"
    );
}

/// The threshold must change what the run does, not merely be accepted. Without this, restoring
/// `"none" => Some(Severity::Error)` leaves every test in this file green.
#[test]
fn a_threshold_changes_the_outcome_on_the_same_bundle() {
    assert_eq!(
        lint_pack(WARNING_PACK, "warn").status.code(),
        Some(1),
        "a warning finding did not gate at --fail-on warn"
    );
    assert_eq!(
        lint_pack(WARNING_PACK, "none").status.code(),
        Some(0),
        "--fail-on none gated a run it was asked not to gate"
    );
    assert_eq!(
        lint_pack(WARNING_PACK, "error").status.code(),
        Some(0),
        "a warning finding gated at --fail-on error, which fires only on errors"
    );
}

/// #2032's acceptance, on the artifact it names: a bundle with warning findings must not exit 0
/// when the threshold it was given does not exist. A zero-finding bundle cannot prove this — it
/// cannot distinguish "rejected the argument" from "accepted it and applied the loosest gate".
#[test]
fn a_warning_bundle_does_not_pass_with_an_unrecognized_threshold() {
    let out = lint_pack(WARNING_PACK, "warnings");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected clap's usage error, got {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
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

/// The `warning` alias by effect rather than by acceptance.
///
/// `every_advertised_spelling_is_accepted` runs on a bundle with no findings, where every valid
/// threshold exits 0, so it cannot tell `warning` from `error`. It only proves the spelling parses.
///
/// The distinction is load-bearing outside this crate. `packs/open/cicd-starter/README.md` tells
/// users to set `fail_on: warning`, and the Action's own bash `case` had no arm for it and no
/// default, so the value gated nothing at all (#2037). The Action now forwards the input here
/// unmodified, which makes this the assertion standing behind that README.
#[test]
fn warning_gates_the_same_findings_as_warn() {
    let warn = lint_pack(WARNING_PACK, "warn").status.code();
    let warning = lint_pack(WARNING_PACK, "warning").status.code();
    let error = lint_pack(WARNING_PACK, "error").status.code();

    assert_eq!(
        warn,
        Some(1),
        "fixture no longer yields a warning finding under {WARNING_PACK}, so the rest proves nothing"
    );
    assert_eq!(
        warning, warn,
        "`warning` is not the alias of `warn` its callers rely on"
    );
    assert_ne!(
        warning, error,
        "`warning` decides what `error` decides, so the alias distinguishes nothing"
    );
}

/// A recipient must be able to identify the source even when no result can carry its identity.
#[test]
fn sarif_retains_bundle_identity_without_results() {
    // Content identity of the checked-in verified fixture, not its gzip file hash or run label.
    const BUNDLE_ID: &str =
        "sha256:7a47bb25f2bd8e4ca031033d6c151812188f3b2bb8fa629ed100ea2fa53b7e41";
    for (pack, cap, expected_results, truncated) in [
        ("cicd-starter", "500", 0, false),
        ("soc2-baseline", "500", 1, false),
        ("soc2-baseline", "0", 0, true),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["evidence", "lint"])
            .arg(fixture_bundle())
            .args([
                "--format",
                "sarif",
                "--fail-on",
                "none",
                "--pack",
                pack,
                "--max-results",
                cap,
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{pack}: {:?}", out.stderr);
        let sarif: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let run = &sarif["runs"][0];
        assert_eq!(run["results"].as_array().unwrap().len(), expected_results);
        assert_eq!(
            run["properties"]["truncated"].as_bool().unwrap_or(false),
            truncated
        );
        assert_eq!(
            run["properties"]["bundle_id"], BUNDLE_ID,
            "{pack}, cap={cap}"
        );
        if expected_results > 0 {
            assert_eq!(run["results"][0]["properties"]["bundle_id"], BUNDLE_ID);
        }
    }
}

#[test]
fn sarif_emits_no_document_for_damaged_or_missing_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let damaged = dir.path().join("damaged.tar.gz");
    let bytes = std::fs::read(fixture_bundle()).unwrap();
    std::fs::write(&damaged, &bytes[..100]).unwrap();
    for bundle in [damaged, dir.path().join("missing.tar.gz")] {
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["evidence", "lint"])
            .arg(bundle)
            .args(["--format", "sarif"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(
            out.stdout.is_empty(),
            "refusal must not resemble a clean report"
        );
        assert!(!out.stderr.is_empty());
    }
}
