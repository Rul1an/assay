use std::io::Write;
use std::path::Path;

use assay_core::errors::Diagnostic;
use assay_core::report::summary::Summary;

use crate::cli::commands::pipeline_error::emit_operator_diagnostic;
use crate::exit_codes::{ReasonCode, RunOutcome};

const EVIDENCE_INTEGRITY_CODES: &[assay_evidence::ErrorCode] = &[
    assay_evidence::ErrorCode::IntegrityManifestHash,
    assay_evidence::ErrorCode::IntegrityEventHash,
    assay_evidence::ErrorCode::IntegrityFileSizeMismatch,
    assay_evidence::ErrorCode::IntegrityRunRootMismatch,
];

const EVIDENCE_UNREADABLE_CODES: &[assay_evidence::ErrorCode] = &[
    assay_evidence::ErrorCode::IntegrityIo,
    assay_evidence::ErrorCode::IntegrityGzip,
    assay_evidence::ErrorCode::IntegrityTar,
];

fn evidence_reason_for_verifier_code(code: assay_evidence::ErrorCode) -> Option<ReasonCode> {
    if EVIDENCE_INTEGRITY_CODES.contains(&code) {
        Some(ReasonCode::EEvidenceIntegrity)
    } else if EVIDENCE_UNREADABLE_CODES.contains(&code) {
        Some(ReasonCode::EEvidenceUnreadable)
    } else {
        None
    }
}

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
    /// successfully read. The shared verifier-code mapping is pinned to the normative boundary by
    /// a mutation-sensitive test below.
    pub(crate) fn evidence_integrity(path: &Path, error: &anyhow::Error) -> Option<Self> {
        let verifier = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<assay_evidence::VerifyError>())?;
        if evidence_reason_for_verifier_code(verifier.code) != Some(ReasonCode::EEvidenceIntegrity)
        {
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
        let verifier = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<assay_evidence::VerifyError>());
        let unreadable = match verifier {
            Some(verifier) => {
                evidence_reason_for_verifier_code(verifier.code)
                    == Some(ReasonCode::EEvidenceUnreadable)
            }
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
    use super::{CliFailure, EVIDENCE_INTEGRITY_CODES, EVIDENCE_UNREADABLE_CODES};
    use assay_evidence::{ErrorClass, ErrorCode, VerifyError};
    use std::collections::BTreeSet;
    use std::path::Path;

    fn verifier_error(class: ErrorClass, code: ErrorCode) -> anyhow::Error {
        anyhow::Error::new(VerifyError::new(class, code, "measured failure"))
            .context("bundle reader failed")
    }

    #[test]
    fn evidence_integrity_code_set_matches_the_normative_boundary() {
        let boundary = include_str!("exit_codes/evidence_integrity_boundary.md");
        let required = boundary
            .split_once("An emitter MUST key on")
            .and_then(|(_, rest)| rest.split_once("and MUST NOT map"))
            .map(|(required, _)| required)
            .expect("normative boundary must retain its required/forbidden code clauses");
        let normative: BTreeSet<&str> = required
            .split('`')
            .filter(|token| token.starts_with("Integrity"))
            .collect();
        let implemented: BTreeSet<String> = EVIDENCE_INTEGRITY_CODES
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            implemented,
            normative.into_iter().map(str::to_string).collect(),
            "the executable integrity classifier drifted from the one normative boundary"
        );
    }

    #[test]
    fn evidence_unreadable_code_set_matches_the_normative_registry() {
        let spec = include_str!("../../../docs/architecture/SPEC-PR-Gate-Outputs-v1.md");
        let row = spec
            .lines()
            .find(|line| line.starts_with("| E_EVIDENCE_UNREADABLE |"))
            .expect("reason registry must retain E_EVIDENCE_UNREADABLE");
        let normative: BTreeSet<&str> = row
            .split('`')
            .filter(|token| token.starts_with("Integrity"))
            .collect();
        let implemented: BTreeSet<String> = EVIDENCE_UNREADABLE_CODES
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            implemented,
            normative.into_iter().map(str::to_string).collect(),
            "the executable unreadable classifier drifted from the normative registry"
        );
    }

    #[test]
    fn evidence_integrity_classification_matches_the_normative_code_boundary() {
        for &code in EVIDENCE_INTEGRITY_CODES {
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

        for &code in EVIDENCE_UNREADABLE_CODES {
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
