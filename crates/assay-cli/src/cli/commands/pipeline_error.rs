use super::reporting::write_error_artifacts;
use super::run_output::reason_code_from_run_error;
use crate::exit_codes::{ExitCodeVersion, ReasonCode};
use assay_core::errors::RunError;
use std::path::Path;
use std::time::Instant;

pub(crate) enum PipelineError {
    Classified { run_error: RunError },
    Fatal(anyhow::Error),
}

pub(crate) fn elapsed_ms(start: Instant) -> u64 {
    let ms = start.elapsed().as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

impl PipelineError {
    pub(crate) fn cfg_parse(path: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::config_parse(Some(path.into()), msg.into()),
        }
    }

    pub(crate) fn missing_cfg(path: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::missing_config(path.into(), msg.into()),
        }
    }

    pub(crate) fn invalid_args(msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::invalid_args(msg.into()),
        }
    }

    pub(crate) fn from_run_error(run_error: RunError) -> Self {
        Self::Classified { run_error }
    }

    pub(crate) fn into_exit_code(
        self,
        version: ExitCodeVersion,
        verify_enabled: bool,
        run_json_path: &Path,
    ) -> anyhow::Result<i32> {
        match self {
            Self::Classified { run_error } => {
                let reason =
                    reason_code_from_run_error(&run_error).unwrap_or(ReasonCode::ECfgParse);
                write_error_artifacts(
                    reason,
                    run_error.message,
                    version,
                    verify_enabled,
                    run_json_path,
                )
            }
            Self::Fatal(err) => Err(err),
        }
    }
}
