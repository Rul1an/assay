use super::super::super::args::{JudgeArgs, RunArgs};
use std::path::PathBuf;

pub(super) fn replay_run_args(
    config: PathBuf,
    trace_file: Option<PathBuf>,
    db: PathBuf,
    replay_strict: bool,
    exit_codes: crate::exit_codes::ExitCodeVersion,
) -> RunArgs {
    let judge = JudgeArgs {
        no_judge: true,
        ..JudgeArgs::default()
    };

    RunArgs {
        config,
        db,
        quarantine_mode: "off".to_string(),
        trace_file,
        refresh_cache: true,
        no_cache: true,
        judge,
        replay_strict,
        exit_codes,
        ..RunArgs::default()
    }
}
