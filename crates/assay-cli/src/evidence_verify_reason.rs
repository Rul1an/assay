//! Command-neutral evidence-error → `ReasonCode` classifier.
//!
//! Typed `VerifyError` is authoritative. Untyped I/O is Unreadable only when no
//! verifier code is present. Profile-invalid is not a verifier-code fact and is
//! not classified here.

use assay_evidence::{ErrorClass, ErrorCode, VerifyError};

use crate::exit_codes::ReasonCode;

/// Map a typed verifier error to the registered evidence reason, or `None` when the
/// class+code pair is outside the normative mapping. Reachability of a code on one
/// command is not this function's question.
pub(crate) fn reason_code_for_verify_error(error: &VerifyError) -> Option<ReasonCode> {
    match (error.class, error.code) {
        (
            ErrorClass::Integrity,
            ErrorCode::IntegrityManifestHash
            | ErrorCode::IntegrityEventHash
            | ErrorCode::IntegrityFileSizeMismatch
            | ErrorCode::IntegrityRunRootMismatch,
        ) => Some(ReasonCode::EEvidenceIntegrity),
        (
            ErrorClass::Integrity,
            ErrorCode::IntegrityIo | ErrorCode::IntegrityGzip | ErrorCode::IntegrityTar,
        ) => Some(ReasonCode::EEvidenceUnreadable),
        (
            ErrorClass::Contract,
            // ContractMissingManifest and ContractTimestampRegression are classifier
            // members. writer_next/verify.rs does not currently construct them.
            ErrorCode::ContractMissingManifest
            | ErrorCode::ContractSchemaVersion
            | ErrorCode::ContractFileOrder
            | ErrorCode::ContractMissingFile
            | ErrorCode::ContractDuplicateFile
            | ErrorCode::ContractUnexpectedFile
            | ErrorCode::ContractRunIdMismatch
            | ErrorCode::ContractBundleIdMismatch
            | ErrorCode::ContractSequenceGap
            | ErrorCode::ContractSequenceStart
            | ErrorCode::ContractTimestampRegression
            | ErrorCode::ContractInvalidJson
            | ErrorCode::ContractInvalidEvent,
        ) => Some(ReasonCode::EEvidenceContract),
        (
            ErrorClass::Limits,
            ErrorCode::LimitBundleBytes
            | ErrorCode::LimitDecodeBytes
            | ErrorCode::LimitFileSize
            | ErrorCode::LimitLineBytes
            | ErrorCode::LimitTotalEvents
            | ErrorCode::LimitPathLength
            | ErrorCode::LimitJsonDepth,
        ) => Some(ReasonCode::EEvidenceLimitExceeded),
        (
            ErrorClass::Security,
            ErrorCode::SecurityPathTraversal | ErrorCode::SecurityAbsolutePath,
        ) => Some(ReasonCode::EEvidencePathRejected),
        _ => None,
    }
}

/// Classify a command-level evidence failure.
///
/// Walk the cause chain for a typed `VerifyError` first. That mapping is
/// authoritative even when an `io::Error` is also present. Only when no
/// verifier code is present does a direct or nested `std::io::Error` become
/// `E_EVIDENCE_UNREADABLE`.
pub(crate) fn reason_code_for_evidence_error(error: &anyhow::Error) -> Option<ReasonCode> {
    if let Some(verifier) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<VerifyError>())
    {
        return reason_code_for_verify_error(verifier);
    }
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
    {
        return Some(ReasonCode::EEvidenceUnreadable);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{reason_code_for_evidence_error, reason_code_for_verify_error};
    use crate::exit_codes::ReasonCode;
    use assay_evidence::{ErrorClass, ErrorCode, VerifyError};

    fn classify(class: ErrorClass, code: ErrorCode) -> Option<ReasonCode> {
        reason_code_for_verify_error(&VerifyError::new(class, code, "synthetic"))
    }

    #[test]
    fn recorded_value_integrity_maps_to_integrity() {
        for code in [
            ErrorCode::IntegrityManifestHash,
            ErrorCode::IntegrityEventHash,
            ErrorCode::IntegrityFileSizeMismatch,
            ErrorCode::IntegrityRunRootMismatch,
        ] {
            assert_eq!(
                classify(ErrorClass::Integrity, code),
                Some(ReasonCode::EEvidenceIntegrity),
                "{code}"
            );
        }
    }

    #[test]
    fn archive_read_integrity_maps_to_unreadable() {
        for code in [
            ErrorCode::IntegrityIo,
            ErrorCode::IntegrityGzip,
            ErrorCode::IntegrityTar,
        ] {
            assert_eq!(
                classify(ErrorClass::Integrity, code),
                Some(ReasonCode::EEvidenceUnreadable),
                "{code}"
            );
        }
    }

    #[test]
    fn contract_star_maps_to_contract_only_with_the_contract_class() {
        for code in [
            ErrorCode::ContractMissingManifest,
            ErrorCode::ContractSchemaVersion,
            ErrorCode::ContractFileOrder,
            ErrorCode::ContractMissingFile,
            ErrorCode::ContractDuplicateFile,
            ErrorCode::ContractUnexpectedFile,
            ErrorCode::ContractRunIdMismatch,
            ErrorCode::ContractBundleIdMismatch,
            ErrorCode::ContractSequenceGap,
            ErrorCode::ContractSequenceStart,
            ErrorCode::ContractTimestampRegression,
            ErrorCode::ContractInvalidJson,
            ErrorCode::ContractInvalidEvent,
        ] {
            assert_eq!(
                classify(ErrorClass::Contract, code),
                Some(ReasonCode::EEvidenceContract),
                "{code}"
            );
            assert_eq!(
                classify(ErrorClass::Integrity, code),
                None,
                "{code} under Integrity must not fold into CONTRACT"
            );
        }
    }

    #[test]
    fn every_reachable_limit_star_maps_to_limit_exceeded() {
        for code in [
            ErrorCode::LimitBundleBytes,
            ErrorCode::LimitDecodeBytes,
            ErrorCode::LimitFileSize,
            ErrorCode::LimitLineBytes,
            ErrorCode::LimitTotalEvents,
            ErrorCode::LimitPathLength,
            ErrorCode::LimitJsonDepth,
        ] {
            assert_eq!(
                classify(ErrorClass::Limits, code),
                Some(ReasonCode::EEvidenceLimitExceeded),
                "{code}"
            );
            assert_ne!(
                classify(ErrorClass::Limits, code),
                Some(ReasonCode::EEvidenceIntegrity)
            );
            assert_ne!(
                classify(ErrorClass::Limits, code),
                Some(ReasonCode::EEvidenceContract)
            );
            assert_ne!(
                classify(ErrorClass::Limits, code),
                Some(ReasonCode::EEvidenceUnreadable)
            );
            assert_eq!(
                classify(ErrorClass::Contract, code),
                None,
                "{code} under Contract must not fold into LIMIT"
            );
        }
    }

    #[test]
    fn every_declared_security_star_maps_to_path_rejected_only_with_the_security_class() {
        for code in [
            ErrorCode::SecurityPathTraversal,
            ErrorCode::SecurityAbsolutePath,
        ] {
            assert_eq!(
                classify(ErrorClass::Security, code),
                Some(ReasonCode::EEvidencePathRejected),
                "{code}"
            );
            assert_ne!(
                classify(ErrorClass::Security, code),
                Some(ReasonCode::EEvidenceLimitExceeded),
                "{code}"
            );
            assert_eq!(
                classify(ErrorClass::Limits, code),
                None,
                "{code} under Limits must not fold into PATH_REJECTED"
            );
        }
    }

    #[test]
    fn untyped_io_without_a_verifier_is_unreadable() {
        let direct = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(
            reason_code_for_evidence_error(&direct),
            Some(ReasonCode::EEvidenceUnreadable)
        );
        let wrapped = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::IsADirectory))
            .context("bundle reader failed");
        assert_eq!(
            reason_code_for_evidence_error(&wrapped),
            Some(ReasonCode::EEvidenceUnreadable)
        );
    }

    #[test]
    fn typed_contract_wrapped_with_io_stays_contract() {
        let err = anyhow::Error::new(
            VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractInvalidJson,
                "invalid event",
            )
            .with_source(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
        );
        assert_eq!(
            reason_code_for_evidence_error(&err),
            Some(ReasonCode::EEvidenceContract)
        );
        assert_ne!(
            reason_code_for_evidence_error(&err),
            Some(ReasonCode::EEvidenceUnreadable)
        );
    }
}
