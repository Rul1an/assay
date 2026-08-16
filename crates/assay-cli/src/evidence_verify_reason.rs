//! Command-neutral `VerifyError` → `ReasonCode` classifier.
//!
//! One function answers the class+code rule for every evidence CLI site. Profile-invalid is
//! not a verifier-code fact and is not classified here.

use assay_evidence::{ErrorClass, ErrorCode, VerifyError};

use crate::exit_codes::ReasonCode;

/// Map a typed verifier error to the registered evidence reason, or `None` when the pair
/// is outside this command's reachable completeness set.
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
        (ErrorClass::Security, ErrorCode::SecurityPathTraversal) => {
            Some(ReasonCode::EEvidencePathRejected)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::reason_code_for_verify_error;
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
    fn security_path_traversal_maps_to_path_rejected() {
        assert_eq!(
            classify(ErrorClass::Security, ErrorCode::SecurityPathTraversal),
            Some(ReasonCode::EEvidencePathRejected)
        );
        assert_ne!(
            classify(ErrorClass::Security, ErrorCode::SecurityPathTraversal),
            Some(ReasonCode::EEvidenceLimitExceeded)
        );
    }

    #[test]
    fn security_absolute_path_is_not_required_for_completeness() {
        assert_eq!(
            classify(ErrorClass::Security, ErrorCode::SecurityAbsolutePath),
            None,
            "SecurityAbsolutePath is enum-only; do not invent an emission"
        );
    }
}
