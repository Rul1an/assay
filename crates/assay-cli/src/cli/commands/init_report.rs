//! What `assay init` did, recorded once and rendered by whichever channel the caller asked for.
//!
//! `init` used to write its progress straight to stdout with `println!`, which left a caller that
//! reads only stdout and the exit status unable to tell a partial failure from a success (#2161).
//! The record here is the single statement of what happened; the text channel renders it as the
//! human progress stream it always was, and the JSON channel renders it as one document. Neither
//! channel recomputes what the other one says.

use std::path::Path;

use crate::cli::args::common::OutputFormat;
use crate::exit_codes::{format_recovery_argv, validate_recovery_argv, RunOutcome};

/// Every successful `init` next step. `succeed` takes this type so a new site
/// cannot publish a hand-rolled argv that the recovery harness never drives.
pub(crate) enum InitSuccess {
    ListPresets,
    Validate {
        config: String,
        replay_trace: Option<String>,
    },
}

impl InitSuccess {
    fn argv(&self) -> Vec<String> {
        match self {
            Self::ListPresets => vec![
                "assay".to_string(),
                "init".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
            Self::Validate {
                config,
                replay_trace,
            } => validate_recovery_argv(config, replay_trace.as_deref()),
        }
    }
}

/// Document identity for the machine channel.
pub(crate) const INIT_REPORT_SCHEMA: &str = "assay.init_report.v0";

/// POSIX separators for the machine channel.
///
/// The same `init` names the same files on every platform, so a consumer can compare a report
/// across runners. The text channel keeps `Path::display`, which is what the user's own shell
/// shows them.
fn posix(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) struct InitReport {
    format: OutputFormat,
    created: Vec<String>,
    skipped: Vec<String>,
    presets: Vec<serde_json::Value>,
}

impl InitReport {
    pub(crate) fn new(format: OutputFormat) -> Self {
        Self {
            format,
            created: Vec::new(),
            skipped: Vec::new(),
            presets: Vec::new(),
        }
    }

    pub(crate) fn is_json(&self) -> bool {
        self.format == OutputFormat::Json
    }

    /// A human progress line. Silent under JSON, so stdout carries exactly one document.
    pub(crate) fn progress(&self, line: &str) {
        if !self.is_json() {
            println!("{line}");
        }
    }

    pub(crate) fn record_created(&mut self, path: &Path, detail: Option<&str>) {
        self.created.push(posix(path));
        match detail {
            Some(detail) => self.progress(&format!("   Created {} ({detail})", path.display())),
            None => self.progress(&format!("   Created {}", path.display())),
        }
    }

    pub(crate) fn record_skipped(&mut self, path: &Path) {
        self.skipped.push(posix(path));
        self.progress(&format!("   Skipped {} (exists)", path.display()));
    }

    pub(crate) fn record_presets(&mut self, presets: &'static [crate::packs::Pack]) {
        for pack in presets {
            self.presets.push(serde_json::json!({
                "name": pack.name,
                "description": pack.description,
            }));
            self.progress(&format!("{}\t{}", pack.name, pack.description));
        }
    }

    /// The human rendering of the command a caller should run next.
    ///
    /// Both channels read the same argv: this joins it the way a user would type it, which is the
    /// line `init` has always printed, and [`Self::succeed`] publishes that same argv as JSON.
    /// Callers place this line themselves, because the surrounding notes differ per entry point
    /// and their order is the output users already have.
    pub(crate) fn next_line(&self, next: &InitSuccess) -> String {
        format!("   Next: {}", next.argv().join(" "))
    }

    /// Ends a successful `init`.
    ///
    /// `human_lines` are the closing text lines in the order the text channel prints them; the
    /// machine channel ignores them and publishes the same argv [`InitSuccess`] built.
    /// Caller-controlled option values are fused (`--config=path`) before they reach
    /// this function, so the machine channel states the step as JSON argv rather
    /// than as a shell string a consumer would have to re-split.
    pub(crate) fn succeed(self, next: &InitSuccess, human_lines: &[String]) -> anyhow::Result<i32> {
        if self.is_json() {
            // A report that could not be rendered leaves the caller with no document, so it exits
            // through the error path rather than returning the success it cannot evidence.
            let argv = next.argv();
            let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            let next_step = format_recovery_argv(&argv_refs);
            self.emit(&RunOutcome::success(), Some(next_step))?;
            return Ok(crate::exit_codes::OK);
        }
        for line in human_lines {
            self.progress(line);
        }
        Ok(crate::exit_codes::OK)
    }

    /// Ends an `init` that failed with a reason the registry names.
    ///
    /// The text channel keeps returning the error so `main` prints the same `fatal:` line on
    /// stderr it always has; only the machine channel is new.
    pub(crate) fn fail(self, outcome: RunOutcome) -> anyhow::Result<i32> {
        if self.is_json() {
            let exit_code = outcome.exit_code;
            let next_step = outcome.next_step.clone();
            self.emit(&outcome, next_step)?;
            return Ok(exit_code);
        }
        Err(anyhow::anyhow!(
            "{}",
            outcome
                .message
                .unwrap_or_else(|| outcome.reason_code.clone())
        ))
    }

    fn emit(self, outcome: &RunOutcome, next_step: Option<String>) -> anyhow::Result<()> {
        let mut document = serde_json::json!({
            "schema": INIT_REPORT_SCHEMA,
            "exit_code": outcome.exit_code,
            "reason_code": outcome.reason_code,
            "created": self.created,
            "skipped": self.skipped,
        });
        let object = document
            .as_object_mut()
            .expect("the document is constructed as an object");
        if let Some(message) = &outcome.message {
            object.insert("message".to_string(), serde_json::json!(message));
        }
        if let Some(next_step) = next_step {
            object.insert("next_step".to_string(), serde_json::json!(next_step));
        }
        if !self.presets.is_empty() {
            object.insert("presets".to_string(), serde_json::json!(self.presets));
        }
        println!("{}", serde_json::to_string_pretty(&document)?);
        Ok(())
    }
}
