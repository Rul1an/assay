use serde::Serialize;

/// Verification error classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Integrity violation (hash mismatch, corrupted gzip/tar).
    Integrity,
    /// Contract violation (missing fields, wrong source format, disallowed files).
    Contract,
    /// Security violation (path traversal, malicious payloads).
    Security,
    /// Resource limit exceeded (DoS prevention).
    Limits,
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Stable error codes for verification failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    // Integrity
    IntegrityGzip,
    IntegrityTar,
    IntegrityManifestHash,
    IntegrityEventHash,
    IntegrityFileSizeMismatch,
    IntegrityRunRootMismatch,
    IntegrityZipBomb,
    IntegrityIo,
    // Contract
    ContractMissingManifest,
    ContractSchemaVersion,
    ContractFileOrder,
    ContractMissingFile,
    ContractDuplicateFile,
    ContractUnexpectedFile,
    ContractRunIdMismatch,
    ContractSequenceGap,
    ContractSequenceStart,
    ContractTimestampRegression,
    ContractInvalidJson,
    ContractInvalidEvent,
    // Limits
    LimitPathLength,
    LimitFileSize,
    LimitTotalEvents,
    LimitLineBytes,
    LimitJsonDepth,
    LimitBundleBytes,
    LimitDecodeBytes,
    // Security
    SecurityPathTraversal,
    SecurityAbsolutePath,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Typed verification error with stable code.
#[derive(Debug, thiserror::Error)]
#[error("{class}: {message} ({code})")]
pub struct VerifyError {
    pub class: ErrorClass,
    pub code: ErrorCode,
    pub message: String,
    #[source]
    pub source: Option<anyhow::Error>,
}

impl VerifyError {
    pub fn new(class: ErrorClass, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            class,
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.message = format!("{}: {}", context.into(), self.message);
        self
    }

    pub fn class(&self) -> ErrorClass {
        self.class
    }
}

// Helper for IO errors (defaults to Integrity/IntegrityIo)
impl From<std::io::Error> for VerifyError {
    fn from(err: std::io::Error) -> Self {
        Self {
            class: ErrorClass::Integrity,
            code: ErrorCode::IntegrityIo,
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}

// Helper for JSON errors (defaults to Contract/ContractSchemaVersion - simplified)
impl From<serde_json::Error> for VerifyError {
    fn from(err: serde_json::Error) -> Self {
        Self {
            class: ErrorClass::Contract,
            code: ErrorCode::ContractSchemaVersion,
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}
