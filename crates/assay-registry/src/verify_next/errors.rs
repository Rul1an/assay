//! Error mapping boundary for Step-2 split.
//!
//! Contract target:
//! - typed error constructors/mapping helpers
//! - deterministic error classification

use crate::error::RegistryError;

pub(super) fn digest_mismatch(
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> RegistryError {
    RegistryError::DigestMismatch {
        name: "pack".to_string(),
        version: "unknown".to_string(),
        expected: expected.into(),
        actual: actual.into(),
    }
}

pub(super) fn unsigned_pack() -> RegistryError {
    RegistryError::Unsigned {
        name: "pack".to_string(),
        version: "unknown".to_string(),
    }
}

pub(super) fn invalid_response(message: impl Into<String>) -> RegistryError {
    RegistryError::InvalidResponse {
        message: message.into(),
    }
}

pub(super) fn signature_invalid(reason: impl Into<String>) -> RegistryError {
    RegistryError::SignatureInvalid {
        reason: reason.into(),
    }
}
