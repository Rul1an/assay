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
        let message = format!("failed to parse policy {path}: {error}");
        let outcome =
            RunOutcome::from_reason(ReasonCode::EPolicyParse, Some(message), Some(path.as_str()));
        Self {
            outcome,
            source: "policy",
            context: serde_json::json!({ "path": path }),
        }
    }

    /// Classify only verifier outcomes that establish a recorded-value mismatch.
    ///
    /// `ErrorClass::Integrity` is intentionally too broad: the evidence crate also assigns it to
    /// I/O, gzip, and tar failures, none of which establish anything about the content that was
    /// successfully read. Keep this list aligned with the normative boundary included on
    /// `ReasonCode::EEvidenceIntegrity`.
    pub(crate) fn evidence_integrity(path: &Path, error: &anyhow::Error) -> Option<Self> {
        use assay_evidence::ErrorCode;

        let verifier = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<assay_evidence::VerifyError>())?;
        if !matches!(
            verifier.code,
            ErrorCode::IntegrityManifestHash
                | ErrorCode::IntegrityEventHash
                | ErrorCode::IntegrityFileSizeMismatch
                | ErrorCode::IntegrityRunRootMismatch
        ) {
            return None;
        }

        let path = path.display().to_string();
        let verifier_code = verifier.code.to_string();
        let message = format!("evidence bundle {path} failed content verification: {error}");
        let outcome = RunOutcome::from_reason(
            ReasonCode::EEvidenceIntegrity,
            Some(message),
            Some(path.as_str()),
        );
        Some(Self {
            outcome,
            source: "evidence",
            context: serde_json::json!({
                "path": path,
                "verifier_code": verifier_code,
            }),
        })
    }

    /// Classify failures where the bundle could not be opened or read to completion.
    ///
    /// If the evidence verifier supplied a typed code, that code is authoritative. Searching the
    /// rest of its source chain for an I/O error would misclassify a contract or content finding
    /// that merely carries an I/O source.
    pub(crate) fn evidence_unreadable(path: &Path, error: &anyhow::Error) -> Option<Self> {
        use assay_evidence::ErrorCode;

        let verifier = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<assay_evidence::VerifyError>());
        let unreadable = match verifier {
            Some(verifier) => matches!(
                verifier.code,
                ErrorCode::IntegrityIo | ErrorCode::IntegrityGzip | ErrorCode::IntegrityTar
            ),
            None => {
                error.downcast_ref::<std::io::Error>().is_some()
                    || error
                        .chain()
                        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
            }
        };
        if !unreadable {
            return None;
        }

        let path = path.display().to_string();
        let message = format!("evidence bundle {path} could not be opened or read: {error}");
        let outcome = RunOutcome::from_reason(
            ReasonCode::EEvidenceUnreadable,
            Some(message),
            Some(path.as_str()),
        );
        Some(Self {
            outcome,
            source: "evidence",
            context: serde_json::json!({ "path": path }),
        })
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

#[cfg(test)]
mod tests {
    use super::CliFailure;
    use assay_evidence::{ErrorClass, ErrorCode, VerifyError};
    use std::path::Path;

    fn verifier_error(class: ErrorClass, code: ErrorCode) -> anyhow::Error {
        anyhow::Error::new(VerifyError::new(class, code, "measured failure"))
            .context("bundle reader failed")
    }

    #[test]
    fn evidence_integrity_classification_matches_the_normative_code_boundary() {
        for code in [
            ErrorCode::IntegrityManifestHash,
            ErrorCode::IntegrityEventHash,
            ErrorCode::IntegrityFileSizeMismatch,
            ErrorCode::IntegrityRunRootMismatch,
        ] {
            let failure = CliFailure::evidence_integrity(
                Path::new("bundle.tar.gz"),
                &verifier_error(ErrorClass::Integrity, code),
            )
            .unwrap_or_else(|| panic!("{code} must classify as an evidence mismatch"));
            assert_eq!(failure.outcome.reason_code, "E_EVIDENCE_INTEGRITY");
            assert_eq!(failure.outcome.exit_code, 2);
            assert!(
                failure
                    .outcome
                    .next_step
                    .as_deref()
                    .is_some_and(|step| !step.is_empty()),
                "{code} must carry remediation"
            );
        }

        for (class, code) in [
            (ErrorClass::Integrity, ErrorCode::IntegrityIo),
            (ErrorClass::Integrity, ErrorCode::IntegrityGzip),
            (ErrorClass::Integrity, ErrorCode::IntegrityTar),
            (ErrorClass::Contract, ErrorCode::ContractInvalidJson),
            (ErrorClass::Limits, ErrorCode::LimitBundleBytes),
            (ErrorClass::Security, ErrorCode::SecurityPathTraversal),
        ] {
            assert!(
                CliFailure::evidence_integrity(
                    Path::new("bundle.tar.gz"),
                    &verifier_error(class, code),
                )
                .is_none(),
                "{code} establishes no recorded-value mismatch"
            );
        }
    }

    #[test]
    fn evidence_unreadable_classification_excludes_content_and_contract_findings() {
        let direct_io = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
        let failure = CliFailure::evidence_unreadable(Path::new("missing.bundle"), &direct_io)
            .expect("a direct open failure must classify as unreadable");
        assert_eq!(failure.outcome.reason_code, "E_EVIDENCE_UNREADABLE");

        for code in [
            ErrorCode::IntegrityIo,
            ErrorCode::IntegrityGzip,
            ErrorCode::IntegrityTar,
        ] {
            assert!(
                CliFailure::evidence_unreadable(
                    Path::new("bundle.tar.gz"),
                    &verifier_error(ErrorClass::Integrity, code),
                )
                .is_some(),
                "{code} must classify as unreadable"
            );
        }

        for (class, code) in [
            (ErrorClass::Integrity, ErrorCode::IntegrityManifestHash),
            (ErrorClass::Contract, ErrorCode::ContractInvalidJson),
            (ErrorClass::Limits, ErrorCode::LimitBundleBytes),
            (ErrorClass::Security, ErrorCode::SecurityPathTraversal),
        ] {
            assert!(
                CliFailure::evidence_unreadable(
                    Path::new("bundle.tar.gz"),
                    &verifier_error(class, code),
                )
                .is_none(),
                "{code} is not an unreadable-bundle finding"
            );
        }

        let contract_with_io_source = anyhow::Error::new(
            VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractInvalidJson,
                "invalid event",
            )
            .with_source(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
        );
        assert!(
            CliFailure::evidence_unreadable(Path::new("bundle.tar.gz"), &contract_with_io_source,)
                .is_none(),
            "a typed contract code must not be reclassified from its nested I/O source"
        );
    }
}
