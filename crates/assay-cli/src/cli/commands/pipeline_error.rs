use super::reporting::write_error_artifacts;
use super::run_output::reason_code_from_run_error;
use crate::exit_codes::{ExitCodeVersion, ReasonCode};
use assay_core::errors::{Diagnostic, RunError, RunErrorKind};
use std::io::{IsTerminal, Write};
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

/// Where the operator should go looking, derived from the error kind rather than
/// from the message text.
fn source_for(run_error: &RunError) -> String {
    match &run_error.kind {
        RunErrorKind::ConfigParse | RunErrorKind::MissingConfig => "config".to_string(),
        RunErrorKind::TraceNotFound => "trace".to_string(),
        RunErrorKind::InvalidArgs => "cli".to_string(),
        RunErrorKind::ProviderRateLimit
        | RunErrorKind::ProviderTimeout
        | RunErrorKind::ProviderServer
        | RunErrorKind::JudgeUnavailable => run_error
            .provider
            .clone()
            .unwrap_or_else(|| "provider".to_string()),
        RunErrorKind::Network => "network".to_string(),
        RunErrorKind::Other => "unknown".to_string(),
    }
}

/// Build the operator-facing diagnostic from the same `ReasonCode` that goes into
/// `run.json`.
///
/// Both the code and the fix step come from `ReasonCode`: `as_str()` and
/// `next_step()`. That is what keeps stderr and the artifacts from drifting. A
/// second, stderr-only code table would be free to disagree with the one frozen
/// in SPEC-PR-Gate-Outputs-v1.
pub(crate) fn diagnostic_for(run_error: &RunError, reason: ReasonCode) -> Diagnostic {
    let mut context = serde_json::Map::new();

    if let Some(path) = &run_error.path {
        context.insert("path".into(), path.clone().into());
    }
    if let Some(provider) = &run_error.provider {
        context.insert("provider".into(), provider.clone().into());
    }
    if let Some(status) = run_error.status {
        context.insert("status".into(), status.into());
    }
    // Several constructors set `detail` from the same string as `message`
    // (`RunError::config_parse` among them), so echo it only when it adds something.
    if let Some(detail) = &run_error.detail {
        if detail != &run_error.message {
            context.insert("detail".into(), detail.clone().into());
        }
    }
    // Say so when the code came from parsing a message rather than from a typed
    // constructor: it is the difference between a classification and a guess.
    if run_error.legacy_classified {
        context.insert("classified_from".into(), "message".into());
    }

    Diagnostic::new(reason.as_str(), run_error.message.clone())
        .with_source(source_for(run_error))
        .with_context(serde_json::Value::Object(context))
        .with_fix_step(reason.next_step(run_error.path.as_deref()))
}

/// Write the diagnostic to stderr, decorated only when stderr is a terminal.
///
/// Returns `()`, not `Result`. The exit code is the gate contract; stderr is an
/// affordance for the human reading the log. A closed pipe must not be able to
/// change the former.
pub(crate) fn emit_operator_diagnostic(diagnostic: &Diagnostic) {
    let stderr = std::io::stderr();
    let decorated = stderr.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let rendered = if decorated {
        diagnostic.format_terminal()
    } else {
        diagnostic.format_plain()
    };
    let _ = stderr.lock().write_all(rendered.as_bytes());
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
                emit_operator_diagnostic(&diagnostic_for(&run_error, reason));
                // Both channels get the same context. `next_step()` interpolates the
                // path, so withholding it here is what made run.json print
                // `<config.yaml>` while stderr named the real file.
                write_error_artifacts(
                    reason,
                    run_error.message.clone(),
                    run_error.path.as_deref(),
                    version,
                    verify_enabled,
                    run_json_path,
                )
            }
            Self::Fatal(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit_codes::RunOutcome;

    fn cases() -> Vec<RunError> {
        vec![
            RunError::config_parse(
                Some("assay.yaml".to_string()),
                "mapping values are not allowed here",
            ),
            RunError::missing_config("assay.yaml", "no config file found"),
            RunError::trace_not_found("traces/run.jsonl", "trace file does not exist"),
            RunError::invalid_args("unrecognized flag --nope"),
        ]
    }

    /// Mirrors exactly what `into_exit_code` hands to `write_error_artifacts`, so
    /// these tests fail if the two channels are ever fed different context again.
    fn outcome_as_written(run_error: &RunError, reason: ReasonCode) -> RunOutcome {
        RunOutcome::from_reason(
            reason,
            Some(run_error.message.clone()),
            run_error.path.as_deref(),
        )
    }

    /// The property this issue exists to establish: an operator grepping stderr and
    /// a CI job parsing run.json see the same code for the same failure.
    #[test]
    fn stderr_code_matches_run_json_reason_code() {
        for run_error in cases() {
            let reason =
                reason_code_from_run_error(&run_error).expect("typed constructors classify");
            let diagnostic = diagnostic_for(&run_error, reason);

            assert_eq!(diagnostic.code, reason.as_str());
            assert_eq!(
                diagnostic.code,
                outcome_as_written(&run_error, reason).reason_code
            );
        }
    }

    /// The fix step is not retyped for stderr; it is the one already in run.json.
    ///
    /// This test previously built the expected outcome with `None` for context,
    /// which is what the buggy production path passed, so it compared the bug
    /// against itself and passed while the two channels printed different steps.
    #[test]
    fn fix_step_matches_run_json_next_step() {
        for run_error in cases() {
            let reason = reason_code_from_run_error(&run_error).unwrap();
            let diagnostic = diagnostic_for(&run_error, reason);
            let outcome = outcome_as_written(&run_error, reason);

            assert_eq!(diagnostic.fix_steps, vec![outcome.next_step.unwrap()]);
        }
    }

    /// The two codes that interpolate context must name the real file on both
    /// channels, not the placeholder.
    #[test]
    fn path_bearing_errors_name_the_real_path_on_both_channels() {
        let config = RunError::config_parse(Some("suites/prod.yaml".to_string()), "bad indent");
        let diagnostic = diagnostic_for(&config, ReasonCode::ECfgParse);
        assert_eq!(
            diagnostic.fix_steps[0],
            "Run: assay doctor --config suites/prod.yaml"
        );
        assert_eq!(
            outcome_as_written(&config, ReasonCode::ECfgParse).next_step,
            Some(diagnostic.fix_steps[0].clone())
        );

        let trace = RunError::trace_not_found("traces/run.jsonl", "missing");
        let diagnostic = diagnostic_for(&trace, ReasonCode::ETraceNotFound);
        assert_eq!(
            diagnostic.fix_steps[0],
            "Check trace file exists: traces/run.jsonl"
        );
        assert_eq!(
            outcome_as_written(&trace, ReasonCode::ETraceNotFound).next_step,
            Some(diagnostic.fix_steps[0].clone())
        );
    }

    /// With no path there is nothing to interpolate, and the placeholder is what
    /// both channels should show.
    #[test]
    fn pathless_config_error_falls_back_to_the_placeholder_on_both_channels() {
        let run_error = RunError::config_parse(None, "could not locate a config");
        let diagnostic = diagnostic_for(&run_error, ReasonCode::ECfgParse);
        assert_eq!(
            diagnostic.fix_steps[0],
            "Run: assay doctor --config <config.yaml>"
        );
        assert_eq!(
            outcome_as_written(&run_error, ReasonCode::ECfgParse).next_step,
            Some(diagnostic.fix_steps[0].clone())
        );
    }

    #[test]
    fn config_parse_context_does_not_repeat_the_message() {
        let run_error = RunError::config_parse(
            Some("assay.yaml".to_string()),
            "mapping values are not allowed here",
        );
        // The constructor sets detail == message; the diagnostic must not print both.
        assert_eq!(run_error.detail.as_deref(), Some(run_error.message.as_str()));

        let diagnostic = diagnostic_for(&run_error, ReasonCode::ECfgParse);
        let context = diagnostic.context.as_object().unwrap();
        assert!(!context.contains_key("detail"));
        assert_eq!(context.get("path").unwrap(), "assay.yaml");
        assert_eq!(diagnostic.source, "config");
    }

    #[test]
    fn detail_is_kept_when_it_adds_something() {
        let run_error = RunError::new(RunErrorKind::ConfigParse, "config rejected")
            .with_detail("line 4, column 11");
        let diagnostic = diagnostic_for(&run_error, ReasonCode::ECfgParse);
        assert_eq!(
            diagnostic.context.get("detail").and_then(|v| v.as_str()),
            Some("line 4, column 11")
        );
    }

    #[test]
    fn message_derived_classification_is_marked_as_such() {
        let typed = RunError::invalid_args("unrecognized flag --nope");
        assert!(!typed.legacy_classified);
        let diagnostic = diagnostic_for(&typed, ReasonCode::EInvalidArgs);
        assert!(!diagnostic
            .context
            .as_object()
            .unwrap()
            .contains_key("classified_from"));

        let mut guessed = typed;
        guessed.legacy_classified = true;
        let diagnostic = diagnostic_for(&guessed, ReasonCode::EInvalidArgs);
        assert_eq!(
            diagnostic
                .context
                .get("classified_from")
                .and_then(|v| v.as_str()),
            Some("message")
        );
    }

    #[test]
    fn provider_errors_name_the_provider_as_the_source() {
        let run_error = RunError::new(RunErrorKind::ProviderServer, "upstream 503")
            .with_provider("openai")
            .with_status(503);
        let diagnostic = diagnostic_for(&run_error, ReasonCode::EProvider5xx);
        assert_eq!(diagnostic.source, "openai");
        assert_eq!(
            diagnostic.context.get("status").and_then(|v| v.as_u64()),
            Some(503)
        );
    }

    /// `RunErrorKind::Other` has no reason code, so `into_exit_code` substitutes
    /// `ECfgParse`. That substitution predates this change and already shapes the
    /// exit code and run.json; pinning the stderr line here so #2010 can see what a
    /// unified registry has to improve. `source: unknown` is the honest part.
    #[test]
    fn unclassified_errors_inherit_the_config_parse_fallback() {
        let run_error = RunError::other("something unexpected happened");
        assert!(reason_code_from_run_error(&run_error).is_none());

        let diagnostic = diagnostic_for(&run_error, ReasonCode::ECfgParse);
        assert_eq!(diagnostic.code, "E_CFG_PARSE");
        assert_eq!(diagnostic.source, "unknown");
    }

    /// Non-TTY rendering goes into CI logs, so the code has to lead the line and the
    /// icon has to be gone.
    #[test]
    fn plain_rendering_leads_with_the_code_and_drops_the_icon() {
        let run_error = RunError::config_parse(
            Some("assay.yaml".to_string()),
            "mapping values are not allowed here",
        );
        let rendered = diagnostic_for(&run_error, ReasonCode::ECfgParse).format_plain();
        assert!(rendered.starts_with("[E_CFG_PARSE]"));
        assert!(!rendered.contains('\u{274c}'));
        assert!(!rendered.contains('\u{26a0}'));
    }

    /// A parser quoting a non-ASCII source line must not stop the code from leading
    /// the line, and must not smuggle the icon back in. An `is_ascii` assertion here
    /// would only be testing the fixture.
    #[test]
    fn non_ascii_messages_still_render_plain() {
        let run_error = RunError::config_parse(
            Some("assay.yaml".to_string()),
            "found character '\u{e9}' that cannot start any token",
        );
        let rendered = diagnostic_for(&run_error, ReasonCode::ECfgParse).format_plain();
        assert!(rendered.starts_with("[E_CFG_PARSE]"));
        assert!(!rendered.contains('\u{274c}'));
        assert!(rendered.contains('\u{e9}'), "the message is preserved");
    }
}
