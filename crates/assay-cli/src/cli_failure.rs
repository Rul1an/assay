use std::io::Write;
use std::path::Path;

use assay_core::errors::Diagnostic;
use assay_core::report::summary::Summary;

use crate::cli::commands::pipeline_error::emit_operator_diagnostic;
use crate::exit_codes::{ReasonCode, RunOutcome};

/// A classified command failure that the top-level CLI funnel can render.
///
/// Untyped `anyhow::Error` values deliberately do not enter this path: assigning a
/// reason code is a command-level decision, not something the funnel can infer from
/// prose without silently misclassifying failures.
#[derive(Debug)]
pub(crate) struct CliFailure {
    outcome: RunOutcome,
    source: &'static str,
    context: serde_json::Value,
}

impl CliFailure {
    pub(crate) fn policy_parse(path: &Path, error: impl std::fmt::Display) -> Self {
        let path = path.display().to_string();
        let message = format!("failed to load policy {path}: {error}");
        let outcome =
            RunOutcome::from_reason(ReasonCode::EPolicyParse, Some(message), Some(path.as_str()));
        Self {
            outcome,
            source: "policy",
            context: serde_json::json!({ "path": path }),
        }
    }

    pub(crate) fn emit(self, machine_output: bool) -> i32 {
        emit_operator_diagnostic(&self.diagnostic());
        if machine_output {
            let summary = summary_from_outcome(&self.outcome, true);
            if let Err(error) = emit_summary_stdout(&summary) {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "WARNING: failed to render CLI failure on stdout: {error}"
                );
            }
        }
        self.outcome.exit_code
    }

    fn diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            self.outcome.reason_code.clone(),
            self.outcome.message.clone().unwrap_or_default(),
        )
        .with_source(self.source)
        .with_context(self.context.clone());
        if let Some(next_step) = &self.outcome.next_step {
            diagnostic = diagnostic.with_fix_step(next_step.clone());
        }
        diagnostic
    }
}

impl std::fmt::Display for CliFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.outcome.message.as_deref().unwrap_or("CLI failure"))
    }
}

impl std::error::Error for CliFailure {}

/// Build the one summary shape shared by run artifacts and top-level CLI failures.
pub(crate) fn summary_from_outcome(outcome: &RunOutcome, verify_enabled: bool) -> Summary {
    let assay_version = env!("CARGO_PKG_VERSION");
    if outcome.exit_code == 0 {
        Summary::success(assay_version, verify_enabled)
    } else {
        Summary::failure(
            outcome.exit_code,
            &outcome.reason_code,
            outcome.message.as_deref().unwrap_or(""),
            outcome.next_step.as_deref().unwrap_or(""),
            assay_version,
            verify_enabled,
        )
    }
}

pub(crate) fn emit_summary_stdout(summary: &Summary) -> anyhow::Result<()> {
    let rendered = assay_core::report::summary::render_summary_json(summary)?;
    writeln!(std::io::stdout().lock(), "{rendered}")?;
    Ok(())
}
