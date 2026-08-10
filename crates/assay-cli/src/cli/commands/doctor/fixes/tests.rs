//! Unit coverage for the one `run_doctor_fix` return no binary-level test can reach.
//!
//! Everything else in this module is pinned end-to-end from
//! `crates/assay-cli/tests/doctor_exit_class_contract.rs`, which is where the exit class belongs:
//! it drives the real binary and compares two runs against each other. The post-repair re-load is
//! the exception, and the reason is worth stating rather than working around.
//!
//! Reaching it needs a config that loads, a repair that applies, and the same config failing to
//! load afterwards. On the doctor path the only repair that can apply is trace creation --
//! `build_suggestions`' patch arms need a `file`/`field` context that `assay_core::validate` does
//! not attach to `E_PATH_NOT_FOUND`, so no patch is ever offered -- and `create_empty_trace` runs
//! only for a path that does not exist. A write that cannot land on an existing file cannot break
//! an existing config, so no argument to `assay doctor` produces the state. The branch still has to
//! answer correctly if it is ever reached, which is what this test pins.

use std::path::Path;

use assay_core::errors::diagnostic::{codes, Diagnostic};

use crate::cli::args::common::OutputFormat;
use crate::cli::args::DoctorArgs;
use crate::exit_codes::{ReasonCode, RunOutcome};

use super::run_doctor_fix;

fn fix_args(config: &Path, trace_file: &Path) -> DoctorArgs {
    DoctorArgs {
        config: Some(config.to_path_buf()),
        trace_file: Some(trace_file.to_path_buf()),
        baseline: None,
        db: None,
        replay_strict: false,
        format: OutputFormat::Text,
        out: None,
        fix: true,
        yes: true,
        dry_run: false,
    }
}

/// A config that no longer loads after a repair is a config fault, at the class the registry gives
/// one.
///
/// This return was a literal `1` while the same condition one function earlier -- an explicit
/// `--config` that does not load -- was `2` from `ReasonCode::ECfgParse`. So whether an unloadable
/// config counted as a config fault depended on how far `--fix` had got before noticing, which is
/// the flag-dependence this branch exists to remove one condition over.
///
/// The expected value is built from the registry here rather than copied from the production call,
/// and it is not written as `2`: the number belongs to the reason code, and a test that spelled it
/// out would be a second place to change if the registry ever moved.
#[tokio::test]
async fn a_config_that_no_longer_loads_after_a_repair_reports_the_registry_class() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("eval.yaml");
    // Stands in for the state a repair would have to have produced. `run_doctor_fix` does not read
    // the config before applying ops, so the branch sees exactly this: ops applied, config gone.
    std::fs::write(&config, "suite: [unterminated\n").expect("write config");
    let trace = dir.path().join("traces/absent.jsonl");

    let args = fix_args(&config, &trace);
    let diagnostics = vec![Diagnostic::new(
        codes::E_PATH_NOT_FOUND,
        format!("Trace file not found: {}", trace.display()),
    )];

    let exit = run_doctor_fix(&args, &config, &diagnostics, false)
        .await
        .expect("run_doctor_fix");

    assert!(
        trace.exists(),
        "the repair did not run, so this test is not driving the post-repair re-load."
    );
    let registry_class = RunOutcome::from_reason(ReasonCode::ECfgParse, None, None).exit_code;
    assert_eq!(
        exit, registry_class,
        "a config that does not load after a repair returned exit {exit}, while the reason code for \
         an unloadable config is exit {registry_class}. One condition, one class, whether or not a \
         repair ran first."
    );
}
