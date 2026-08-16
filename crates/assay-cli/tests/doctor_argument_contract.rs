//! `assay doctor` argument rejections use the invalid-args class (#2208).
//!
//! Three combinations are refused by the command rather than by clap, and each used to return
//! the test-failure class (`1`) with empty stdout. The same binary, refusing `--format nonsense`
//! one layer earlier, already returns the config/usage class (`2`). The class must come from
//! `ReasonCode::EInvalidArgs`, not from a literal, and must not depend on which layer refused.
//!
//! All three process channels are watched. `--fix --format json` asked for Doctor's machine
//! document, which `diagnostics::report` names `assay.doctor_report.v0`; that request gets
//! exactly one such report on stdout, empty stderr, and the invalid-args exit. `--yes` and
//! `--dry-run` did not request a machine format, so they keep the human rejection line and
//! empty stdout.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::process::{Command, Output};

const DOCTOR_REPORT_SCHEMA: &str = "assay.doctor_report.v0";

fn doctor(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command.env("NO_COLOR", "1").arg("doctor").args(args);
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay doctor").expect("doctor ran")
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("doctor exited by code rather than by signal")
}

/// Parses the whole of stdout, not a fragment of it.
fn sole_report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor --fix --format json stdout is not one JSON document: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn fix_format_json_publishes_the_doctor_report_invalid_args_outcome() {
    let output = doctor(&["--fix", "--format", "json"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        exit_code(&output),
        2,
        "doctor --fix --format json must return the invalid-args class; stderr={stderr}"
    );
    assert!(
        output.stderr.is_empty(),
        "machine rejection must not also write a human line; stderr={stderr}"
    );
    let document = sole_report(&output);
    assert_eq!(
        document["schema"], DOCTOR_REPORT_SCHEMA,
        "machine rejection must stay on Doctor's existing report identity; stdout={stdout}"
    );
    assert_eq!(document["reason_code"], "E_INVALID_ARGS");
    let next_step = document["next_step"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    assert!(
        !next_step.is_empty(),
        "doctor report must carry a non-empty next_step; document={document}"
    );
    assert_eq!(
        document["config_check"]["status"], "skipped",
        "invalid args must not be reported as a config that was read; document={document}"
    );
    let skip_reason = document["config_check"]["reason"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !skip_reason.trim().is_empty(),
        "a skipped check must name why it was skipped; document={document}"
    );
    assert_ne!(
        skip_reason, "No config found; run inside project or use --config",
        "skipped because the combination is invalid, not because no config was found"
    );
    assert!(
        skip_reason.contains("--fix") || skip_reason.contains("format"),
        "skipped reason must name the invalid combination; reason={skip_reason:?}"
    );
}

#[test]
fn text_channel_rejections_keep_prose_and_empty_stdout() {
    for args in [&["--yes"][..], &["--dry-run"][..]] {
        let output = doctor(args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            exit_code(&output),
            2,
            "doctor {} must return the invalid-args class; stderr={stderr}",
            args.join(" ")
        );
        assert!(
            output.stdout.is_empty(),
            "doctor {} requested no machine format; stdout={stdout}",
            args.join(" ")
        );
        assert!(
            stderr.contains("doctor: --yes/--dry-run require --fix"),
            "doctor {} must keep the existing rejection line; stderr={stderr}",
            args.join(" ")
        );
    }
}

/// Clap already refuses `doctor --format nonsense` with the config/usage class. The three
/// command-layer refusals must share that process exit, not merely the literal `2` today's
/// registry maps `EInvalidArgs` to — a remapping that updated the literals here would still
/// leave clap and the command disagreeing.
#[test]
fn command_rejections_share_claps_invalid_format_class() {
    let clap_exit = exit_code(&doctor(&["--format", "nonsense"]));
    for args in [
        &["--fix", "--format", "json"][..],
        &["--yes"][..],
        &["--dry-run"][..],
    ] {
        let got = exit_code(&doctor(args));
        assert_eq!(
            got,
            clap_exit,
            "doctor {} must share clap's class for doctor --format nonsense ({clap_exit})",
            args.join(" ")
        );
    }
}
